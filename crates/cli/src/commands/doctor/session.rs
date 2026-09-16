//! `yunta doctor --session`: one real session per binding.
//!
//! A `probe()` answers whether the CLI is there, answers `--version`
//! and authenticates. It cannot answer whether a session opens, and
//! that is the question a run actually asks — a CLI that refuses the
//! configuration this engine writes it says so on stderr and exits
//! before its first line, and `--version` never touches that
//! configuration.
//!
//! So this opens the smallest run there is: one `kind: prompt` node,
//! run tools mounted, driven through the very machinery a workflow is
//! driven through, in the sandbox `yunta test` runs a case in. One run
//! per binding rather than one run with a node per binding, because a
//! sick adapter refuses the whole invocation: a binding that dies has
//! to be reported as itself.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use yunta_core::events::{Failure, NodeEvent, SessionDeath};
use yunta_core::{RunnerCandidate, RunnerName};

use crate::commands::test::{init_git, sandboxed_checkout, SandboxedCheckout};
use crate::context::Context;
use crate::error::CliError;

/// The workflow every probe run drives: one node, one prompt, the run
/// tools a real node is given. Nothing is asked of the agent beyond
/// answering, because the verdict is whether the session opened at all.
const PROBE_WORKFLOW: &str = r#"
name: doctor-session
nodes:
  - id: probe
    kind: prompt
    runner: probe
    prompt: "Reply with the single word: ok."
"#;

/// The runner [`PROBE_WORKFLOW`] declares. Only this one is left in the
/// sandbox's config, so the run resolves to exactly the binding under
/// test and `probe_or_refuse` probes exactly its adapter.
const PROBE_RUNNER: RunnerName = RunnerName::from_static("probe");

/// One binding, and every runner that reaches it.
pub(super) struct Binding {
    pub(super) candidate: RunnerCandidate,
    /// Each runner that names it, and whether it names it first. A
    /// binding a runner falls back to is worth as much as its first:
    /// a run reaches it exactly when the first one is down.
    pub(super) named_by: Vec<(RunnerName, bool)>,
}

/// Every distinct binding any runner names, in the order a reader meets
/// them: by adapter, then model, then agent.
///
/// The binding and not the runner name, because a session exercises a
/// binding: two runners naming the same one are not two things to test,
/// and a binding held in reserve is.
pub(super) fn bindings(config: &yunta_core::ConfigLayer) -> Vec<Binding> {
    let mut found: BTreeMap<(String, String, String), Binding> = BTreeMap::new();
    for (runner, candidates) in config.runners.iter().flatten() {
        for (position, candidate) in candidates.iter().enumerate() {
            let key = (
                candidate.adapter.to_string(),
                candidate.model.to_string(),
                candidate
                    .agent
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            );
            found
                .entry(key)
                .or_insert_with(|| Binding {
                    candidate: candidate.clone(),
                    named_by: Vec::new(),
                })
                .named_by
                .push((runner.clone(), position == 0));
        }
    }
    found.into_values().collect()
}

/// How one binding's probe run ended.
pub(super) struct SessionProbe {
    binding: String,
    named_by: String,
    outcome: Outcome,
}

enum Outcome {
    /// The session opened, ran and closed its turn.
    Ok { tokens: u64 },
    /// The session ended without ever reporting a terminal event.
    Died(Box<SessionDeath>),
    /// Everything else: the node failed for its own reasons, or the run
    /// could not be built at all.
    Refused(String),
}

impl SessionProbe {
    /// Whether this is something a person has to act on.
    pub(super) fn is_ok(&self) -> bool {
        matches!(self.outcome, Outcome::Ok { .. })
    }
}

impl fmt::Display for SessionProbe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): ", self.binding, self.named_by)?;
        match &self.outcome {
            Outcome::Ok { tokens } => {
                write!(
                    f,
                    "ok — {}",
                    yunta_core::text::counted(*tokens as usize, "token")
                )
            }
            Outcome::Died(died) => write!(f, "session died — {died}"),
            Outcome::Refused(why) => write!(f, "no session — {why}"),
        }
    }
}

/// Opens one session on `binding` and reports how it ended.
pub(super) async fn session_probe(ctx: &Context, binding: &Binding) -> SessionProbe {
    let named = |first: bool| if first { "" } else { " fallback" };
    let probe = |outcome| SessionProbe {
        binding: label(&binding.candidate),
        named_by: binding
            .named_by
            .iter()
            .map(|(runner, first)| format!("{runner}{}", named(*first)))
            .collect::<Vec<_>>()
            .join(", "),
        outcome,
    };
    match drive_probe(ctx, &binding.candidate).await {
        Ok(outcome) => probe(outcome),
        Err(e) => probe(Outcome::Refused(e.to_string())),
    }
}

/// `adapter/model` or `adapter/model/agent` — what a reader matches back
/// to a line of their own `runners:`.
fn label(candidate: &RunnerCandidate) -> String {
    match &candidate.agent {
        Some(agent) => format!("{}/{}/{agent}", candidate.adapter, candidate.model),
        None => format!("{}/{}", candidate.adapter, candidate.model),
    }
}

/// The whole run, from sandbox to verdict.
async fn drive_probe(ctx: &Context, candidate: &RunnerCandidate) -> Result<Outcome, CliError> {
    let sandbox = sandboxed_checkout(&ctx.cwd)?;
    let workflow = sandbox.worktree().join("doctor-session.yaml");
    tokio::fs::write(&workflow, PROBE_WORKFLOW)
        .await
        .map_err(|e| CliError::io("write", workflow.display(), e))?;
    init_git(sandbox.worktree(), ctx.supervision()).await?;
    let ctx = probe_context(ctx, &sandbox, candidate);

    let storage = ctx.async_storage().await?;
    let (frozen, adapters) =
        crate::commands::run::runnable(&ctx, &workflow, &[], None, None).await?;
    let manifest = frozen.manifest.clone();
    let prepared = crate::commands::run::create_run_from(&ctx, &storage, &frozen, None).await?;
    let report = crate::commands::drive::execute(crate::commands::drive::Executing {
        run_id: &prepared.run_id,
        manifest: &manifest,
        run_dir: &prepared.run_dir,
        worktree: &prepared.worktree,
        adapters: &adapters,
        storage: &storage,
        clock: Arc::new(ctx.clock),
        ids: &ctx.ids,
        // Nothing in this workflow asks anybody anything.
        human_interaction: &yunta_engine::NoInteraction,
        forge: None,
        cancel: ctx.cancellation(),
        adapter_override: None,
        observer: None,
        fence_hook: ctx.fence_hook.clone(),
        ambient: &ctx.env,
    })
    .await?;

    let events = storage.events_for_run(prepared.run_id.clone()).await?;
    Ok(verdict(&events, &report))
}

/// What the run's own log says happened to the one node.
fn verdict(
    events: &[yunta_core::events::StoredEvent],
    report: &yunta_engine::RunReport,
) -> Outcome {
    let failure = events.iter().rev().find_map(|event| match event.payload() {
        Some(yunta_core::events::EventPayload::Node(NodeEvent::Failed(p))) => {
            Some(p.failure.clone())
        }
        _ => None,
    });
    match failure {
        Some(Failure::SessionDied { died }) => Outcome::Died(Box::new(died)),
        Some(other) => Outcome::Refused(other.to_string()),
        None => match &report.terminal {
            yunta_engine::RunTerminal::Finished => Outcome::Ok {
                tokens: report
                    .state
                    .run
                    .closed_tokens()
                    .map(|tokens| tokens.total())
                    .unwrap_or_default(),
            },
            other => Outcome::Refused(format!("{other:?}")),
        },
    }
}

/// The context a probe run drives in: the sandbox `yunta test` builds,
/// over this project's real config narrowed to the one binding under
/// test and with `baseline:` removed — a probe asks whether a session
/// opens, never what the tree measured.
fn probe_context(
    ctx: &Context,
    sandbox: &SandboxedCheckout,
    candidate: &RunnerCandidate,
) -> Context {
    let mut probing = ctx.sandboxed(sandbox.worktree().to_path_buf(), sandbox.root());
    probing.project.config.runners = Some(
        [(PROBE_RUNNER, vec![candidate.clone()])]
            .into_iter()
            .collect(),
    );
    probing.project.config.baseline = None;
    probing
}
