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
use std::sync::Arc;

use yunta_core::events::{ArtifactId, Failure, PauseReason, SessionDeath};
use yunta_core::{
    AdapterId, ArtifactKind, NodeId, RunTool, RunnerCandidate, RunnerName, ToolNaming,
};

use crate::commands::test::{init_git, sandboxed_checkout, SandboxedCheckout};
use crate::context::Context;
use crate::error::CliError;
use crate::render::blocks::{Check, Found};

/// The workflow every probe run drives: one node, one prompt, the run
/// tools a real node is given. The verdict requires the submission to be
/// accepted and the run to finish.
fn probe_workflow(prompt: &str) -> String {
    format!(
        r#"
name: doctor-session
nodes:
  - id: probe
    kind: prompt
    runner: probe
    prompt: '{prompt}'
    artifacts:
      produces: [questions]
"#
    )
}

/// The submit tool's name as `adapter` calls it: the name its session is
/// given, with the prefix its CLI mounts its tools under. An adapter this
/// binary does not build has no naming of its own, so the bare name stands in.
fn submit_called(adapter: &AdapterId) -> String {
    let naming = crate::commands::built_adapter(adapter)
        .map_or(ToolNaming::Bare, |built| built.capabilities().tool_naming);
    RunTool::Submit(ArtifactKind::Questions).called(naming)
}

/// The runner the probe workflow declares. Only this one is left in the
/// sandbox's config, so the run resolves to exactly the binding under
/// test and `probe_or_refuse` probes exactly its adapter.
const PROBE_RUNNER: RunnerName = RunnerName::from_static("probe");

/// The node the probe workflow declares; the probe reads its facts off it.
const PROBE_NODE: NodeId = NodeId::from_static("probe");

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
    /// The probe as a check: the binding and the runners that name it,
    /// and how its session ended.
    pub(super) fn check(&self) -> Check {
        let (found, said) = match &self.outcome {
            Outcome::Ok { tokens } => (
                Found::Holds,
                format!("ok — {}", yunta_core::units::Tokens(*tokens)),
            ),
            Outcome::Died(died) => (
                Found::Problem,
                format!(
                    "session died — {}",
                    crate::render::failure::session_death(died)
                ),
            ),
            Outcome::NoDelivery(why) => (
                Found::Problem,
                format!("session opened, no questions document — {why}"),
            ),
            Outcome::Refused(why) => (Found::Problem, format!("probe failed — {why}")),
        };
        Check {
            found,
            subject: format!("{} ({})", self.binding, self.named_by),
            said,
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
    let submit = submit_called(&candidate.adapter);
    let text = probe_workflow(&format!(
        "Call {submit} with exactly {{\"document\":{{\"questions\":[]}}}}. Then finish."
    ));
    tokio::fs::write(&workflow, text)
        .await
        .map_err(|e| CliError::io("write", workflow.display(), e))?;
    init_git(sandbox.worktree(), ctx.supervision()).await?;
    let ctx = probe_context(ctx, &sandbox, candidate);

    let storage = ctx.async_storage().await?;
    let frozen =
        crate::commands::run::runnable(&ctx, &workflow, &[], None, None, None, &ctx.env).await?;
    let manifest = frozen.manifest.clone();
    let prepared = crate::commands::run::create_run_from(&ctx, &storage, &frozen, None).await?;
    let adapters = crate::commands::registry::adapters_for(
        &manifest,
        &crate::commands::registry::ScriptedPaths {
            run_dir: &prepared.run_dir,
            worktree: &prepared.worktree,
        },
    )?;
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
        observer: None,
        rail_command: ctx.rail_command.clone(),
        ambient: &ctx.env,
    })
    .await?;

    Ok(verdict(&report))
}

/// What the run's own log says happened to the one node: the state the
/// report carries is that log as the run left it.
fn verdict(report: &yunta_engine::RunReport) -> Outcome {
    let facts = ProbeFacts::of(&report.state);
    match facts.failure {
        Some(Failure::SessionDied { died }) => Outcome::Died(Box::new(died)),
        other if facts.opened && !facts.accepted => Outcome::NoDelivery(other.map_or_else(
            || "the run ended without artifact_accepted".to_string(),
            |failure| crate::render::failure::sentence(&failure),
        )),
        Some(other) => Outcome::Refused(crate::render::failure::sentence(&other)),
        None => match &report.terminal {
            yunta_engine::RunTerminal::Finished
                if facts.accepted && facts.node_finished && facts.run_finished =>
            {
                Outcome::Ok {
                    tokens: report.state.metrics().tokens.total(),
                }
            }
            other => Outcome::Refused(format!(
                "the run ended {} without a complete accepted submission",
                ended(other)
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
    /// The facts a probe run's derived state carries: the failure the run's
    /// frame names, and what the probe node and its questions artifact show.
    fn of(state: &yunta_engine::RunState) -> Self {
        let record = state.nodes.get(&PROBE_NODE);
        let questions = ArtifactId::Interpreted {
            kind: ArtifactKind::Questions,
        };
        Self {
            failure: state.last_failure(),
            opened: record.is_some_and(|record| record.last_session.is_some()),
            accepted: state
                .artifacts
                .latest(&questions, Some(&PROBE_NODE))
                .is_some(),
            node_finished: record.is_some_and(|record| record.last_finished.is_some()),
            run_finished: state.run.finished().is_some(),
        }
    }

    #[cfg(test)]
    fn from_events(events: &[yunta_core::events::StoredEvent]) -> Self {
        Self::of(&yunta_engine::derive(events))
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

/// How a run ended, said the way the rest of a refusal reads.
fn ended(terminal: &yunta_engine::RunTerminal) -> String {
    match terminal {
        yunta_engine::RunTerminal::Finished => "finished".to_string(),
        yunta_engine::RunTerminal::Paused { reason } => {
            format!("waiting ({})", yunta_render::pause::sentence(reason))
        }
        yunta_engine::RunTerminal::Failed { node, failure } => {
            let said = PauseReason::NodeFailed {
                node: node.clone(),
                failure: failure.clone(),
            };
            format!("failed ({})", yunta_render::pause::sentence(&said))
        }
        yunta_engine::RunTerminal::Promoted { suggested_mode } => {
            format!("promoted to mode `{suggested_mode}`")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::{
        AgentSessionOpenedPayload, ArtifactAcceptedPayload, ArtifactEvent, EventPayload, NodeEvent,
        NodeFailedPayload, NodeFinishedPayload, NodeOutcome, NodeStartedPayload, RecordedOrigin,
        RunEvent, RunFinishedPayload, RunMetrics, SessionEvent, StoredEvent, TerminalState,
        TokenUsage,
    };
    use yunta_core::{RunId, Workflow};
    use yunta_engine::RunPhase;
    use yunta_testkit_core::{fixed_now, Log};

    fn node_started(attempt: u32) -> EventPayload {
        EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(attempt)))
    }

    fn node_failed(text: &str) -> EventPayload {
        EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
            Failure::Message {
                outcome: text.into(),
            },
            TokenUsage::default(),
        )))
    }

    fn node_finished() -> EventPayload {
        EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
            NodeOutcome::Summary("ok".into()),
            TokenUsage::default(),
        )))
    }

    fn run_failed() -> EventPayload {
        EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
            terminal_state: TerminalState::Failed,
            metrics: RunMetrics {
                cptv: None,
                tokens: TokenUsage::default(),
            },
            closed_by: None,
        }))
    }

    /// A finished run's report over `events`, as the run left its log.
    fn finished_report(events: &[StoredEvent]) -> yunta_engine::RunReport {
        yunta_engine::RunReport {
            terminal: yunta_engine::RunTerminal::Finished,
            state: yunta_engine::derive(events),
        }
    }

    #[test]
    fn accepted_questions_and_both_terminals_are_required_for_success() {
        let opened = EventPayload::Session(SessionEvent::Opened(AgentSessionOpenedPayload {
            session_id: "doctor-session".into(),
            agent: None,
            effort: None,
            model: None,
            capabilities: yunta_core::Capabilities::default(),
            fence: None,
            confinement: None,
            task_id: None,
            continues: None,
        }));
        let accepted =
            EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
                ArtifactId::Interpreted {
                    kind: yunta_core::ArtifactKind::Questions,
                },
                yunta_core::sha256_hex(b"questions: []\n"),
                RecordedOrigin::Submitted,
            )));
        let finished = node_finished();
        let closed = EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
            terminal_state: TerminalState::Done,
            metrics: RunMetrics {
                cptv: None,
                tokens: TokenUsage::default(),
            },
            closed_by: None,
        }));
        let only_reply = Log::for_run("doctor-test")
            .node("probe", opened.clone())
            .build();
        assert!(matches!(
            verdict(&finished_report(&only_reply)),
            Outcome::NoDelivery(_)
        ));
        let delivered = Log::for_run("doctor-test")
            .node("probe", node_started(1))
            .node("probe", opened)
            .node("probe", accepted)
            .node("probe", finished)
            .event(closed)
            .build();
        assert!(matches!(
            verdict(&finished_report(&delivered)),
            Outcome::Ok { .. }
        ));
    }

    #[test]
    fn doctor_and_the_phase_page_name_the_same_last_failure() {
        // "build" failed and stayed failed. "probe" failed later, then ran again
        // to its finish. The page names the failure still standing; doctor must too.
        let events = Log::for_run("doctor-test")
            .node("build", node_started(1))
            .node("build", node_failed("build broke"))
            .node("probe", node_started(1))
            .node("probe", node_failed("probe broke"))
            .node("probe", node_started(2))
            .node("probe", node_finished())
            .event(run_failed())
            .build();
        let workflow: Workflow = yunta_core::yaml::parse(
            "name: doctor-test\nnodes:\n  - { id: build, kind: bash, run: \"true\" }\n  - { id: probe, kind: bash, run: \"true\" }\n",
        )
        .unwrap();
        let frame = yunta_engine::run_frame(
            &RunId::from("doctor-test"),
            &workflow,
            &events,
            None,
            fixed_now(),
        );
        let RunPhase::Failed { failure: page } = frame.phase else {
            panic!("the run closed failed");
        };
        assert_eq!(ProbeFacts::from_events(&events).failure, page);
    }

    #[test]
    fn the_probe_names_the_submit_tool_the_way_its_adapter_calls_it() {
        let claude = yunta_adapters::CLAUDE_CODE_ID.clone();
        let text = probe_workflow(&format!("Call {} with exactly", submit_called(&claude)));
        assert!(text.contains("prompt: 'Call mcp__yunta-run__"), "{text}");
    }

    #[test]
    fn an_adapter_this_binary_does_not_build_names_the_submit_tool_bare() {
        let unbuilt = AdapterId::from_static("unbuilt");
        assert_eq!(
            submit_called(&unbuilt),
            RunTool::Submit(ArtifactKind::Questions).name()
        );
    }
}
