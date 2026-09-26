//! `yunta doctor --session`: one real session per binding.
//!
//! A `probe()` answers whether the CLI is there, answers `--version`
//! and authenticates. It cannot answer whether a session opens, and
//! that is the question a run actually asks — a CLI that refuses the
//! configuration this engine writes it says so on stderr and exits
//! before its first line, and `--version` never touches that
//! configuration.
//!
//! So this opens the smallest run there is: one `kind: prompt` node
//! that submits an empty questions document, driven through the machinery a workflow is
//! driven through, in the sandbox `yunta test` runs a case in. One run
//! per binding rather than one run with a node per binding, because a
//! sick adapter refuses the whole invocation: a binding that dies has
//! to be reported as itself.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use yunta_core::events::{
    ArtifactEvent, ArtifactId, Failure, NodeEvent, RunEvent, SessionDeath, SessionEvent,
};
use yunta_core::{RunnerCandidate, RunnerName};

use crate::commands::test::{init_git, sandboxed_checkout, SandboxedCheckout};
use crate::context::Context;
use crate::error::CliError;

/// The workflow every probe run drives: one node, one prompt, the run
/// tools a real node is given. The verdict requires the submission to be
/// accepted and the run to finish.
const PROBE_WORKFLOW: &str = r#"
name: doctor-session
nodes:
  - id: probe
    kind: prompt
    runner: probe
    prompt: 'Call yunta_submit_questions with exactly {"document":{"questions":[]}}. Then finish.'
    artifacts:
      produces: [questions]
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
    /// The CLI opened but did not hand over the required document.
    NoDelivery(String),
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
            Outcome::NoDelivery(why) => write!(f, "session opened, no questions document — {why}"),
            Outcome::Refused(why) => write!(f, "probe failed — {why}"),
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
        // An empty questions document needs no human response.
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
    let facts = ProbeFacts::from_events(events);
    match facts.failure {
        Some(Failure::SessionDied { died }) => Outcome::Died(Box::new(died)),
        other if facts.opened && !facts.accepted => Outcome::NoDelivery(other.map_or_else(
            || "the run ended without artifact_accepted".to_string(),
            |failure| failure.to_string(),
        )),
        Some(other) => Outcome::Refused(other.to_string()),
        None => match &report.terminal {
            yunta_engine::RunTerminal::Finished
                if facts.accepted && facts.node_finished && facts.run_finished =>
            {
                Outcome::Ok {
                    tokens: report
                        .state
                        .run
                        .closed_tokens()
                        .map(|tokens| tokens.total())
                        .unwrap_or_default(),
                }
            }
            other => Outcome::Refused(format!(
                "run ended as {other:?} without a complete accepted submission"
            )),
        },
    }
}

struct ProbeFacts {
    failure: Option<Failure>,
    opened: bool,
    accepted: bool,
    node_finished: bool,
    run_finished: bool,
}

impl ProbeFacts {
    fn from_events(events: &[yunta_core::events::StoredEvent]) -> Self {
        let failure = events.iter().rev().find_map(|event| match event.payload() {
            Some(yunta_core::events::EventPayload::Node(NodeEvent::Failed(p))) => {
                Some(p.failure.clone())
            }
            _ => None,
        });
        let opened = events.iter().any(|event| {
            matches!(
                event.payload(),
                Some(yunta_core::events::EventPayload::Session(
                    SessionEvent::Opened(_)
                ))
            )
        });
        let accepted = events.iter().any(|event| matches!(event.payload(),
            Some(yunta_core::events::EventPayload::Artifacts(ArtifactEvent::Accepted(p)))
                if event.node_id.as_ref().is_some_and(|id| id.as_str() == "probe")
                    && p.artifact == ArtifactId::Interpreted { kind: yunta_core::ArtifactKind::Questions }
        ));
        let node_finished = events.iter().any(|event| {
            matches!(event.payload(),
            Some(yunta_core::events::EventPayload::Node(NodeEvent::Finished(_)))
                if event.node_id.as_ref().is_some_and(|id| id.as_str() == "probe"))
        });
        let run_finished = events.iter().any(|event| {
            matches!(
                event.payload(),
                Some(yunta_core::events::EventPayload::Run(RunEvent::Finished(_)))
            )
        });
        Self {
            failure,
            opened,
            accepted,
            node_finished,
            run_finished,
        }
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
    // The probe node names its runner explicitly. The project's default
    // may name a runner removed by the narrowed probe config.
    if let Some(defaults) = probing.project.config.defaults.as_mut() {
        defaults.runner = None;
    }
    probing.project.config.baseline = None;
    probing
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::{
        AgentSessionOpenedPayload, ArtifactAcceptedPayload, EventPayload, NodeFinishedPayload,
        RecordedOrigin, RunFinishedPayload, RunMetrics, TerminalState, TokenUsage,
    };
    use yunta_testkit_core::Log;

    #[test]
    fn accepted_questions_and_both_terminals_are_required_for_success() {
        let opened = EventPayload::Session(SessionEvent::Opened(AgentSessionOpenedPayload {
            session_id: "doctor-session".into(),
            agent: None,
            model: None,
            capabilities: yunta_core::Capabilities::default(),
            fence: None,
        }));
        let accepted =
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Questions,
                },
                yunta_core::sha256_hex(b"questions: []\n"),
                RecordedOrigin::Submitted,
            )));
        let finished = EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
            "done",
            TokenUsage::default(),
        )));
        let closed = EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
            terminal_state: TerminalState::Done,
            metrics: RunMetrics {
                cptv: None,
                tokens: TokenUsage::default(),
            },
        }));
        let report = yunta_engine::RunReport {
            terminal: yunta_engine::RunTerminal::Finished,
            state: yunta_engine::RunState::default(),
        };
        let only_reply = Log::for_run("doctor-test")
            .node("probe", opened.clone())
            .build();
        assert!(matches!(
            verdict(&only_reply, &report),
            Outcome::NoDelivery(_)
        ));
        let delivered = Log::for_run("doctor-test")
            .node("probe", opened)
            .node("probe", accepted)
            .node("probe", finished)
            .event(closed)
            .build();
        assert!(matches!(verdict(&delivered, &report), Outcome::Ok { .. }));
    }
}
