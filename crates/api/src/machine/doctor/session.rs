//! See [`super`]. `doctor --session`: one real session per binding.
//!
//! A `probe()` answers whether the CLI is there and answers `--version`.
//! It cannot answer whether a session opens or its credentials hold, and
//! that is the question a run actually asks — a CLI that refuses the
//! configuration this engine writes it says so on stderr and exits
//! before its first line, and `--version` never touches that
//! configuration.
//!
//! So this opens the smallest run there is: one `kind: prompt` node
//! that submits an empty questions document, driven through the machinery a
//! workflow is driven through, in a sandbox of its own. One run
//! per binding rather than one run with a node per binding, because a
//! sick adapter refuses the whole invocation: a binding that dies has
//! to be reported as itself.

use std::collections::BTreeMap;

use yunta_core::events::{ArtifactId, Failure};
use yunta_core::{
    AdapterId, ArtifactKind, NodeId, RunTool, RunnerCandidate, RunnerName, ToolNaming,
};
use yunta_render::page::doctor::{Finding, ProbeEnded, SessionOutcome};

use crate::remark::Unestimated;
use crate::sandbox::Sandbox;
use crate::{Ending, Error, Ports, Project, Remarks, RunRequest, Stop};

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
    let naming = crate::machine::built_adapter(adapter)
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

/// Opens one session on `binding` and finds how it ended. What its run
/// says while it is admitted, created and driven is said through `remarks`,
/// but the estimate: a probe has none worth a line.
pub(super) async fn session_probe(
    project: &Project,
    binding: &Binding,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Finding {
    let said = Unestimated(remarks);
    let outcome = match drive_probe(project, &binding.candidate, &said, stop).await {
        Ok(outcome) => outcome,
        Err(e) => SessionOutcome::Unbuilt {
            said: e.to_string(),
        },
    };
    Finding::Session {
        candidate: binding.candidate.clone(),
        named_by: binding.named_by.clone(),
        outcome,
    }
}

/// The whole run, from sandbox to verdict.
async fn drive_probe(
    project: &Project,
    candidate: &RunnerCandidate,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<SessionOutcome, Error> {
    let sandbox = Sandbox::seeded(&project.workspace.dir)?;
    let workflow = sandbox.worktree().join("doctor-session.yaml");
    let submit = submit_called(&candidate.adapter);
    let text = probe_workflow(&format!(
        "Call {submit} with exactly {{\"document\":{{\"questions\":[]}}}}. Then finish."
    ));
    tokio::fs::write(&workflow, text).await.map_err(|source| {
        let path = workflow.clone();
        crate::Failure::new(ProbeUnwritten { path, source })
    })?;
    sandbox
        .commit_seed(project.workspace.supervision(stop))
        .await?;
    let probing = probe_project(project, &sandbox, candidate);
    let request = RunRequest {
        workflow: workflow.clone(),
        inputs: Default::default(),
        mode: None,
        adapter: None,
        hermetic: false,
    };
    let admission = probing.admit(request, remarks, stop).await?;
    let created = admission.create(remarks, stop).await?;
    let held = created.hold(stop).await?;
    let ports = Ports {
        // An empty questions document needs no human response.
        person: &yunta_core::port::NoInteraction,
        observer: None,
    };
    let driven = held.drive(ports, remarks, stop).await?;
    Ok(verdict(&driven.state, &driven.ending))
}

/// What the run's own log says happened to the one node: `state` is that log
/// as the run left it, and `ending` how it stopped.
fn verdict(state: &yunta_replay::RunState, ending: &Ending) -> SessionOutcome {
    let facts = ProbeFacts::of(state);
    match facts.failure {
        Some(Failure::SessionDied { died }) => SessionOutcome::Died(Box::new(died)),
        failure if facts.opened && !facts.accepted => SessionOutcome::NoDelivery { failure },
        Some(failure) => SessionOutcome::Failed { failure },
        None => match ending {
            Ending::Finished if facts.accepted && facts.node_finished && facts.run_finished => {
                SessionOutcome::Ok {
                    tokens: state.metrics().tokens.total(),
                }
            }
            other => SessionOutcome::Incomplete {
                ended: ended(other),
            },
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
    fn of(state: &yunta_replay::RunState) -> Self {
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
        Self::of(&yunta_replay::RunState::replay(events))
    }
}

/// The project a probe run drives in: this one made in `sandbox`, its
/// config narrowed to the one binding under test and with `baseline:`
/// removed: a probe asks whether a session opens, never what the tree
/// measured.
fn probe_project(project: &Project, sandbox: &Sandbox, candidate: &RunnerCandidate) -> Project {
    let mut probing = project.sandboxed(sandbox);
    probing.config.runners = Some(
        [(PROBE_RUNNER, vec![candidate.clone()])]
            .into_iter()
            .collect(),
    );
    // The probe node names its runner explicitly. The project's default
    // may name a runner removed by the narrowed probe config.
    if let Some(defaults) = probing.config.defaults.as_mut() {
        defaults.runner = None;
    }
    probing.config.baseline = None;
    probing
}

/// How a probe run ended, as its finding holds it.
fn ended(ending: &Ending) -> ProbeEnded {
    match ending {
        Ending::Finished => ProbeEnded::Finished,
        Ending::Paused { reason } => ProbeEnded::Paused {
            reason: reason.clone(),
        },
        Ending::Failed { node, failure } => ProbeEnded::Failed {
            node: node.clone(),
            failure: failure.clone(),
        },
    }
}

/// The probe's workflow the disk would not take: where, and its words.
#[derive(Debug, thiserror::Error)]
#[error("failed to write {}: {source}", path.display())]
struct ProbeUnwritten {
    path: std::path::PathBuf,
    source: std::io::Error,
}

#[cfg(test)]
mod tests;
