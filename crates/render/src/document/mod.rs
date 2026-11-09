//! The versioned JSON Yunta writes for machines: `stats --json`,
//! `status --json`, `run --json`, and the same documents the `yunta mcp`
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

use crate::spend::SpendJson;
use chrono::{DateTime, Utc};
use yunta_core::caution::Caution;
use yunta_core::events::{ChangesCountedPayload, GateWaitingPayload, StoredEvent};
use yunta_core::{Manifest, RunId};
use yunta_replay::frame::{RunFrame, RunPhase};
use yunta_replay::RunState;

pub mod check;
pub mod decision;
mod diagnostics;
mod error;
mod facts;
pub mod field;
mod node;
pub mod stats;
mod waiting;

pub use error::{ErrorDocument, NextCommand};

use diagnostics::{node_diagnostics, ArtifactProblems};
use facts::{BranchJson, CountsJson, PullRequestJson};
use node::NodeJson;
use waiting::WaitingOnJson;

use crate::command::{self, Reader};
use decision::DecisionJson;
use stats::TimeJson;
use surco::words::Standing;

/// What the invocation that creates a run says before the first token —
/// on stderr for the person watching, and carried here so the document
/// that invocation prints says it too: a `--json` reader, or a client of
/// the control plane, is watching nothing else.
#[derive(Debug, Clone, Default)]
pub struct PreRunWarnings {
    /// Every caution the invocation raised ahead of the first token, in the
    /// order it raised them.
    pub cautions: Vec<Caution>,
}

impl PreRunWarnings {
    /// Every caution, one sentence each, in the order they were raised.
    pub fn lines(&self) -> Vec<String> {
        self.cautions.iter().map(crate::caution::line).collect()
    }
}

/// The version stamped on every machine-readable document this CLI emits.
/// Bumped when a field's meaning changes, never for an additive one, so a
/// reader can refuse a document from a schema it predates.
///
/// One number covers every `--json` document together, so a bump earned
/// by one of them re-stamps all of them: a document whose own shape did
/// not change still carries the new number.
pub const SCHEMA_VERSION: u32 = 9;

/// The code a run's word maps to, which a script that started the run reads
/// instead of parsing its output and every document of a run carries: 0 for a
/// run that finished, 3 for one that stopped on a person, 4 for one that
/// finished holding blocking findings, 5 for one parked until a limit resets,
/// 1 for every other stop. One mapping, because "did the run succeed" is one
/// question.
pub fn exit_code(standing: Standing) -> u8 {
    match standing {
        Standing::Finished => 0,
        Standing::NeedsYou => 3,
        Standing::Reported => 4,
        Standing::Waiting => 5,
        Standing::Created
        | Standing::Running
        | Standing::Stalled
        | Standing::Failed
        | Standing::Rerouted
        | Standing::Stopped
        | Standing::Promoted
        | Standing::Broken
        | Standing::Skipped
        | Standing::NeverRan => 1,
    }
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
/// saw `needs you` on a terminal finds `needs you` in the document.
#[derive(serde::Serialize)]
pub struct RunDocument {
    schema_version: u32,
    run_id: String,
    /// What every line a person reads calls the run by.
    handle: String,
    /// The frozen workflow's own name.
    workflow: String,
    /// The mode it runs in.
    mode: String,
    /// How the run stands, in the one vocabulary every surface uses.
    outcome: Standing,
    /// The same standing in a sentence, with the counters a person reads
    /// beside it.
    summary: String,
    /// The sentence under the headline of the run's page. Absent where the word says it all.
    #[serde(skip_serializing_if = "Option::is_none")]
    said: Option<String>,
    /// How far it has come, in numbers.
    counts: CountsJson,
    /// What the invocation that drove it to this word exits with.
    exit_code: u8,
    /// Why the run stopped where it did, when `outcome` alone does not
    /// say it. A parked run says it under `waiting_on` instead, and a
    /// finished one has nothing to add.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// The cautions this document carries: each one a fact a person can
    /// act on ahead of the run's spend. Derived from the run's frame on
    /// every surface, so a parked run carries the budget caution it stopped
    /// on wherever it is read. The invocation that created the run adds
    /// the pre-run estimation to them.
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
    /// What the tree held of the shapes of each plan the run holds, as the
    /// engine read it when the plan was registered: for each shape, whether
    /// its file held it, its code there, and the files beyond the plan whose
    /// code names it. One per plan, in the order the run first accepted
    /// them; absent when the run read none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    shapes_read: Vec<yunta_core::events::ShapesReadPayload>,
    /// Why each failed node failed, in the form a program can act on
    /// rather than parse back out of a sentence: one entry per declared
    /// artifact that did not close, carrying what went wrong with it.
    /// Absent when no failing node named an artifact.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    diagnostics: BTreeMap<String, Vec<ArtifactProblems>>,
    /// The decision this run is parked on: the node it belongs to, when one
    /// owns it, the escalation under the field names the `gate_waiting` event
    /// writes, and the command that answers it.
    ///
    /// Absent for a run that is not parked, and for a pause that
    /// reconstructs no menu — a scope expansion, an unanswered questions
    /// artifact, an external gate with no reachable forge, a parent waiting
    /// on its child. Those pauses are answered where they were raised, and
    /// `waiting_on` carries what the run is waiting on.
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<DecisionJson>,
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
    /// Its wall-clock in seconds. Absent for an empty log.
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_secs: Option<f64>,
    /// Where that time went, by the shares `stats --json` names.
    time: TimeJson,
    /// What it changed from its base to its newest tree, where that was counted.
    #[serde(skip_serializing_if = "Option::is_none")]
    changed: Option<ChangesCountedPayload>,
    /// Where its work is.
    branch: BranchJson,
    /// The pull requests it opened.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pull_requests: Vec<PullRequestJson>,
    /// How many artifacts its nodes wrote.
    artifacts: usize,
    /// What a program can run next, each command with what it does: what a person reads at
    /// the end of the page, naming the run by its whole id.
    next: Vec<NextCommand>,
}

/// A run paused on a limit's reset, as the document carries it.
#[derive(serde::Serialize)]
pub struct AwaitingResetJson {
    node: String,
    at: DateTime<Utc>,
}

impl RunDocument {
    /// The run as its own log describes it. `frame` is the run as the caller framed it at one
    /// instant, with the history it read and what the run's registry says about the process
    /// driving it. `state` is the run derived from `events`; the log is read again only for the
    /// failures each node's diagnostics name.
    pub fn of(
        run_id: &RunId,
        events: &[StoredEvent],
        state: &RunState,
        manifest: &Manifest,
        frame: &RunFrame,
        decision: Option<(
            yunta_core::events::DecisionOn,
            yunta_core::events::Escalation,
        )>,
    ) -> Self {
        let escalation = parked_escalation(frame, decision.as_ref());
        RunDocument {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            handle: run_id.handle().to_string(),
            workflow: frame.workflow.to_string(),
            mode: frame.mode.to_string(),
            outcome: frame.standing,
            // A document reads the same whichever terminal asked for it.
            summary: crate::progress::summary(frame, surco::glyphs::Glyphs::Unicode),
            said: crate::state::gist(frame),
            counts: CountsJson::of(frame),
            exit_code: exit_code(frame.standing),
            reason: reason(&frame.phase),
            cautions: frame.cautions(),
            budget_warning: None,
            context_warnings: Vec::new(),
            blocking_findings: frame.blocking_findings,
            nodes: nodes_of(frame, state),
            tasks: tasks_of(state),
            shapes_read: shapes_read(state),
            diagnostics: node_diagnostics(events),
            decision: parked_decision(run_id, decision, &frame.phase),
            waiting_on: WaitingOnJson::of(frame),
            awaiting_reset: awaiting_reset_of(frame),
            tokens: SpendJson::of(&state.spend()),
            elapsed_secs: frame.elapsed.map(|elapsed| elapsed.as_secs_f64()),
            time: TimeJson::of(&frame.time),
            changed: state.current_change().cloned(),
            branch: BranchJson::of(manifest, run_id),
            pull_requests: PullRequestJson::all_of(frame),
            artifacts: frame.artifacts_written(),
            next: next_of(frame, escalation.as_ref()),
        }
        .with_sentences()
    }

    /// The same document carrying what was said before the run's first
    /// token, added to the cautions the run's frame already holds.
    pub fn warnings(mut self, pre_run: PreRunWarnings) -> Self {
        for caution in pre_run.cautions {
            if !self.cautions.contains(&caution) {
                self.cautions.push(caution);
            }
        }
        self.with_sentences()
    }

    /// The two sentences the cautions carry, read off the cautions so a reader of the list
    /// and a reader of the sentences see one fact.
    fn with_sentences(mut self) -> Self {
        self.budget_warning = self
            .cautions
            .iter()
            .find(|caution| matches!(caution, Caution::BudgetBelowHistory { .. }))
            .map(crate::caution::line);
        self.context_warnings = self
            .cautions
            .iter()
            .filter(|caution| matches!(caution, Caution::ContextFileMissing { .. }))
            .map(crate::caution::line)
            .collect();
        self
    }

    /// The word the run stands at, which the code an invocation that drove
    /// it exits with is read from ([`exit_code`]).
    pub fn outcome(&self) -> Standing {
        self.outcome
    }
}

/// Every node the run's frozen workflow declares, in its declaration order, each with what
/// its log says of it.
fn nodes_of(frame: &RunFrame, state: &RunState) -> Vec<NodeJson> {
    frame
        .nodes
        .iter()
        .map(|node| NodeJson::of(node, state.nodes.get(&node.id)))
        .collect()
}

/// Each task the run registered, by its id, with the word its ledger calls it by.
fn tasks_of(state: &RunState) -> BTreeMap<String, &'static str> {
    state
        .tasks
        .iter()
        .map(|(id, record)| (id.to_string(), record.status.as_str()))
        .collect()
}

/// The escalation a run parked on a person waits on, as its page reads it. `None` for a run
/// that is not parked, whatever decision the log rebuilt.
fn parked_escalation(
    frame: &RunFrame,
    decision: Option<&(
        yunta_core::events::DecisionOn,
        yunta_core::events::Escalation,
    )>,
) -> Option<GateWaitingPayload> {
    decision
        .filter(|_| frame.phase.waiting_on().is_some())
        .map(|(_, escalation)| escalation.clone().into_payload())
}

/// The reset a run paused on a limit's reset waits for, for the document to carry.
fn awaiting_reset_of(frame: &RunFrame) -> Option<AwaitingResetJson> {
    frame
        .phase
        .reset_wait()
        .map(|(node, at)| AwaitingResetJson {
            node: node.to_string(),
            at,
        })
}

/// What a program types next for the run, each command with what it does: the steps a
/// person reads at the end of the run's page, naming the run by its whole id.
fn next_of(frame: &RunFrame, escalation: Option<&GateWaitingPayload>) -> Vec<NextCommand> {
    frame
        .actions(escalation)
        .iter()
        .map(|action| {
            let (line, does) = command::said(action, Reader::Agent);
            NextCommand::new(line, does)
        })
        .collect()
}

/// What the tree held of the shapes of each plan `state` holds, in the
/// order the run first accepted them: the reading the log states for each
/// plan's bytes. A plan none of whose shapes was read has none.
fn shapes_read(state: &RunState) -> Vec<yunta_core::events::ShapesReadPayload> {
    state
        .artifacts
        .of_kind(yunta_core::ArtifactKind::Tasks)
        .filter_map(|held| state.tasks.shapes_read(&held.content_hash))
        .cloned()
        .collect()
}

/// Why the run stopped where it did, for the phases whose word alone
/// does not say it: what broke a failed run, and what stopped making
/// sense in a broken one.
fn reason(phase: &RunPhase) -> Option<String> {
    match phase {
        RunPhase::Failed { failure } => failure
            .as_ref()
            .map(|failure| surco::text::one_line(&crate::failure::sentence(failure))),
        RunPhase::Broken { diagnostic } => Some(surco::text::one_line(diagnostic)),
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
    decision: Option<(
        yunta_core::events::DecisionOn,
        yunta_core::events::Escalation,
    )>,
    phase: &RunPhase,
) -> Option<DecisionJson> {
    if !matches!(phase, RunPhase::Waiting { .. }) {
        return None;
    }
    let (on, escalation) = decision?;
    Some(DecisionJson::new(run_id, &on, escalation.into_payload()))
}
