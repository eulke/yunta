//! The versioned JSON the CLI emits for machines: `stats --json`,
//! `status --json`, `run --json`, and the same DTOs the `yunta mcp`
//! control plane returns in a tool result. One schema version spans
//! them, so a reader keys off a single number that changes only when a
//! field's meaning does — the CLI's own text output is for people, this
//! is the contract for programs.
//!
//! A run is one document here, [`RunDocument`], whichever command
//! produced it: `yunta run` reporting the stop it drove to, `yunta
//! resume` reporting the stop it picked up, `yunta status` reporting
//! where a run stands right now. They are one document because they are
//! one question, and a program that learned to read the answer from one
//! command reads it from the next.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use yunta_core::caution::Caution;
use yunta_core::events::StoredEvent;
use yunta_core::{Manifest, RunId};
use yunta_engine::{RunPhase, RunState, WaitingOn};
use yunta_render::spend::SpendJson;

mod diagnostics;
mod node;

use diagnostics::{node_diagnostics, ArtifactProblems};
use node::{NodeJson, NodeWaitJson};

use crate::commands::advice;
use crate::commands::status::{decision, progress};
use crate::error::{CliError, Outcome};
use crate::render::state::RunWord;

/// What the invocation that creates a run says before the first token —
/// on stderr for the person watching, and carried here so the document
/// that invocation prints says it too: a `--json` reader, or a client of
/// the control plane, is watching nothing else.
#[derive(Debug, Clone, Default)]
pub(crate) struct PreRunWarnings {
    /// Every caution the invocation raised ahead of the first token, in the
    /// order it raised them.
    pub(crate) cautions: Vec<Caution>,
}

impl PreRunWarnings {
    /// Every caution, one sentence each, in the order they were raised.
    pub(crate) fn lines(&self) -> Vec<String> {
        self.cautions
            .iter()
            .map(yunta_render::caution::line)
            .collect()
    }
}

/// The version stamped on every machine-readable document this CLI emits.
/// Bumped when a field's meaning changes, never for an additive one, so a
/// reader can refuse a document from a schema it predates.
///
/// One number covers every `--json` document together, so a bump earned
/// by one of them re-stamps all of them: a document whose own shape did
/// not change still carries the new number.
pub const SCHEMA_VERSION: u32 = 7;

/// Serializes a DTO as pretty JSON to stdout — the one place a `--json`
/// command prints its document, reporting a serialization failure as the
/// one error it can hit rather than unwrapping it.
pub fn print_json<T: serde::Serialize>(value: &T) -> Result<Outcome, CliError> {
    println!("{}", to_json_string(value).map_err(CliError::msg)?);
    Ok(Outcome::Success)
}

/// Serializes a DTO as pretty JSON to a string — what the control plane
/// hands back in a tool result instead of printing, so a `println!` never
/// lands in the middle of its stdio JSON-RPC stream.
pub fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| format!("could not serialize output as JSON: {e}"))
}

/// Where a run stands, as the one versioned document every surface that
/// reports a run emits: `yunta run --json`, `yunta resume --json`,
/// `yunta status --json`, and the `workflow_status` control-plane tool.
///
/// It is derived from the run's own event log and nothing else, so the
/// command that drove a run to its stop and the command that reads that
/// run an hour later publish the same answer. `outcome` is the word
/// every text surface prints, spelled the same way here: a reader who
/// saw `needs you` on a terminal finds `needs_you` in the document.
#[derive(serde::Serialize)]
pub(crate) struct RunDocument {
    schema_version: u32,
    run_id: String,
    /// What every line a person reads calls the run by.
    handle: String,
    /// How the run stands, in the one vocabulary every surface uses.
    outcome: RunWord,
    /// The same standing in a sentence, with the counters a person reads
    /// beside it.
    summary: String,
    /// Why the run stopped where it did, when `outcome` alone does not
    /// say it. A parked run says it under `waiting_on` instead, and a
    /// finished one has nothing to add.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// The cautions this document carries: each one a fact a person can
    /// act on ahead of the run's spend. Carried only by an invocation that
    /// holds them — the one that *created* the run, which is the only one
    /// that has the pre-run estimation, or a status that derives the budget
    /// caution a parked run stopped on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cautions: Vec<Caution>,
    /// The sentence for the budget caution in `cautions`: the declared cap
    /// sits under what this workflow has historically spent. Derived from
    /// `cautions`, so the sentence and the caution say one fact.
    #[serde(skip_serializing_if = "Option::is_none")]
    budget_warning: Option<String>,
    /// The sentence for each `files:` caution in `cautions`, in order: a
    /// path a node reads that the tree this run started from did not hold,
    /// a node that stops there unless something ahead of it writes the file.
    /// Derived from `cautions`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    context_warnings: Vec<String>,
    /// How many findings stand blocking now. A run that finished
    /// carrying any of them is work nobody has accepted, and the count
    /// is what says so to a reader with only this document.
    blocking_findings: usize,
    /// Every node the run's frozen workflow declares, in declaration
    /// order, each `parallel` group followed by its own children — the
    /// same list, in the same order, that `status` prints and that
    /// `graph --run` draws. A list, not a map: the order is a fact
    /// about the workflow, and a map cannot carry one.
    nodes: Vec<NodeJson>,
    tasks: BTreeMap<String, &'static str>,
    /// Why each failed node failed, in the form a program can act on
    /// rather than parse back out of a sentence: one entry per declared
    /// artifact that did not close, carrying what went wrong with it.
    /// Absent when no failing node named an artifact.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    diagnostics: BTreeMap<String, Vec<ArtifactProblems>>,
    /// The decision this run is parked on: the node it belongs to, the
    /// escalation under the field names the `gate_waiting` event writes,
    /// and the command that answers it.
    ///
    /// Absent for a run that is not parked, and for a pause that
    /// reconstructs no menu — a budget cap, a scope expansion, an
    /// unanswered questions artifact, an external gate with no reachable
    /// forge. Those pauses are answered where they were raised, and
    /// `waiting_on` carries what the run is waiting on.
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<decision::DecisionJson>,
    /// What the run is parked on, for every parked run — including the
    /// pauses `decision` is absent for. `summary` says it in a sentence;
    /// this says it in a shape a program acts on without parsing one.
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting_on: Option<WaitingOnJson>,
    /// The reset a limit named, for a run paused on one: the node it is
    /// for, and the moment the run resumes by itself. A person has nothing
    /// to answer before then. Absent for every other run.
    #[serde(skip_serializing_if = "Option::is_none")]
    awaiting_reset: Option<AwaitingResetJson>,
    /// What the run spent: the work its attempts settled, plus the usage
    /// an open attempt has reported that no settled round has taken in.
    tokens: SpendJson,
}

/// A run paused on a limit's reset, as the document carries it.
#[derive(serde::Serialize)]
pub(crate) struct AwaitingResetJson {
    node: String,
    at: DateTime<Utc>,
}

impl RunDocument {
    /// The run as its own log describes it at `now`, which the caller's
    /// injected clock decides, and as `engine` — what the run's registry
    /// says about the process driving it — qualifies a run that looks
    /// like it is moving. `state` is the run derived from `events`; the
    /// log is read again only for the failures each node's diagnostics
    /// name.
    pub(crate) fn of(
        run_id: &RunId,
        events: &[StoredEvent],
        state: &RunState,
        manifest: &Manifest,
        now: DateTime<Utc>,
        engine: &crate::render::Engine,
        decision: Option<(yunta_core::NodeId, yunta_core::events::Escalation)>,
    ) -> Self {
        let frame = progress::frame_of(run_id, manifest, state, now);
        RunDocument {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            handle: run_id.handle().to_string(),
            outcome: crate::render::observed_word(&frame, engine),
            // A document reads the same whichever terminal asked for it.
            summary: progress::summary(&frame, engine, crate::render::Glyphs::Unicode),
            reason: reason(&frame.phase),
            cautions: Vec::new(),
            budget_warning: None,
            context_warnings: Vec::new(),
            blocking_findings: frame.blocking_findings,
            nodes: frame
                .nodes
                .iter()
                .map(|node| NodeJson::of(node, state.nodes.get(&node.id)))
                .collect(),
            tasks: state
                .tasks
                .iter()
                .map(|(id, record)| (id.to_string(), record.status.as_str()))
                .collect(),
            diagnostics: node_diagnostics(events),
            decision: parked_decision(run_id, decision, &frame.phase),
            waiting_on: WaitingOnJson::of(&frame.phase, engine),
            awaiting_reset: advice::reset_pause(&frame.phase).map(|(node, at)| AwaitingResetJson {
                node: node.to_string(),
                at,
            }),
            tokens: SpendJson::of(&state.spend()),
        }
    }

    /// The same document carrying what was said before the run's first
    /// token. The two sentences are read off the cautions, so a reader of
    /// the list and a reader of the sentences see one fact.
    pub(crate) fn warnings(mut self, pre_run: PreRunWarnings) -> Self {
        self.budget_warning = pre_run
            .cautions
            .iter()
            .find(|caution| matches!(caution, Caution::BudgetBelowHistory { .. }))
            .map(yunta_render::caution::line);
        self.context_warnings = pre_run
            .cautions
            .iter()
            .filter(|caution| matches!(caution, Caution::ContextFileMissing { .. }))
            .map(yunta_render::caution::line)
            .collect();
        self.cautions = pre_run.cautions;
        self
    }

    /// What an invocation that drove this run to this word reports, by
    /// the one mapping every surface uses.
    pub(crate) fn verdict(&self) -> Outcome {
        crate::render::exit(self.outcome)
    }
}

/// Why the run stopped where it did, for the phases whose word alone
/// does not say it: what broke a failed run, and what stopped making
/// sense in a broken one.
fn reason(phase: &RunPhase) -> Option<String> {
    match phase {
        RunPhase::Failed { failure } => failure
            .as_ref()
            .map(|failure| yunta_core::text::one_line(&crate::render::failure::sentence(failure))),
        RunPhase::Broken { diagnostic } => Some(yunta_core::text::one_line(diagnostic)),
        RunPhase::Created
        | RunPhase::Running
        | RunPhase::Waiting { .. }
        | RunPhase::Finished
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. } => None,
    }
}

/// The decision a parked run waits on, for a program to read: the same
/// escalation the text view prints, from the same reconstruction, so a
/// client that answers through `resolve_gate` chooses from exactly the
/// menu a person sees. `None` for a run that is not parked and for a
/// pause that reconstructs no menu.
fn parked_decision(
    run_id: &RunId,
    decision: Option<(yunta_core::NodeId, yunta_core::events::Escalation)>,
    phase: &RunPhase,
) -> Option<decision::DecisionJson> {
    if !matches!(phase, RunPhase::Waiting { .. }) {
        return None;
    }
    let (node, escalation) = decision?;
    Some(decision::DecisionJson::new(
        run_id,
        &node,
        escalation.into_payload(),
    ))
}

/// The decision a run waits on, rebuilt from the run's derived state and
/// the documents its gate shows — the one reading every surface takes.
pub(crate) async fn decision_of(
    manifest: &Manifest,
    run_dir: &std::path::Path,
    state: &RunState,
) -> Result<Option<(yunta_core::NodeId, yunta_core::events::Escalation)>, crate::error::CliError> {
    Ok(yunta_engine::current_escalation(manifest, run_dir, state).await?)
}

/// The documents `escalation` shows, as the run whose derived state is
/// `state` holds them.
pub(crate) async fn shown_of(
    run_dir: &std::path::Path,
    escalation: &yunta_core::events::GateWaitingPayload,
    state: &RunState,
) -> Result<Vec<yunta_engine::ShownDocument>, crate::error::CliError> {
    Ok(yunta_engine::shown_documents(run_dir, escalation.shows(), state).await?)
}

/// What a waiting run is waiting on.
///
/// Tagged by `on`, so a reader matches on the shape instead of
/// inferring it from which fields are set: a node parked on a person is
/// a different thing from the run itself stopping, and only one of them
/// has a node to name.
#[derive(serde::Serialize)]
#[serde(tag = "on", rename_all = "snake_case")]
pub(crate) enum WaitingOnJson {
    /// A node is parked on a person. With several parked at once this
    /// is the first in the workflow's declaration order.
    Node {
        node: String,
        /// What that node waits on, in the same shape a node of
        /// `nodes` carries.
        #[serde(flatten)]
        waiting: NodeWaitJson,
        /// What the run's standing pause recorded, absent for a node
        /// parked while the run itself keeps moving.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The run itself paused, with the reason it recorded.
    Run { reason: String },
    /// The engine driving the run is asking a person at its terminal,
    /// where the only answer it takes is given.
    Prompt {
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<String>,
        since: chrono::DateTime<chrono::Utc>,
        pid: yunta_core::Pid,
    },
}

impl WaitingOnJson {
    /// The frame's own answer, in the document's shape — or, for a run
    /// that is moving, the question its engine is asking at its terminal.
    /// `None` for a run that is not waiting on anything.
    fn of(phase: &RunPhase, engine: &crate::render::Engine) -> Option<Self> {
        let RunPhase::Waiting { on } = phase else {
            let moving = matches!(phase, RunPhase::Created | RunPhase::Running);
            return engine
                .prompt
                .as_ref()
                .filter(|_| moving)
                .map(|prompt| WaitingOnJson::Prompt {
                    node: prompt.node.as_ref().map(ToString::to_string),
                    since: prompt.since,
                    pid: prompt.pid,
                });
        };
        Some(match on {
            WaitingOn::Node { node, on, reason } => WaitingOnJson::Node {
                node: node.to_string(),
                waiting: NodeWaitJson::of(on),
                reason: reason.as_ref().map(yunta_render::pause::sentence),
            },
            WaitingOn::Run { reason } => WaitingOnJson::Run {
                reason: yunta_render::pause::sentence(reason),
            },
        })
    }
}
