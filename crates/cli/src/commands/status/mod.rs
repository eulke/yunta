//! `yunta status <run_id>`: where a run stands, derived from the event
//! log alone, never from an estimate or an agent's own report. Two
//! levels — **flow** (the DAG's nodes, over the ones this run's mode
//! schedules) and **task** (tasks done/total) — presented as counters
//! with context, never a percentage: a percentage lies the moment a
//! reroute grows the denominator.
//!
//! A run parked on a person is answerable from here: the decision it
//! stopped on is rebuilt from its own log ([`decision`]) and printed
//! with the command that answers it, so nobody has to read the exported
//! JSONL to learn what the options are.

pub(crate) mod decision;
pub(crate) mod progress;

use yunta_core::events::{Failure, StoredEvent, TaskStatus};
use yunta_core::Clock;
use yunta_core::{Manifest, NodeId, RunId};
use yunta_engine::{NodeState, RunFrame, RunPhase};

use crate::commands::advice;
use crate::context::Context;
use crate::error::note;
use crate::error::{CliError, Outcome};
use crate::render::{indent, NodeDisplay, CHILD_DEPTH, INDENT};

pub async fn status(run_id: &RunId, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let events = open.events;
    // What this binary did not understand in a file a later one wrote:
    // said, because a reader acting on a manifest whose newer half is
    // invisible to them should know that is what they are doing.
    if !open.manifest.unknown.is_empty() {
        note(format!(
            "this run's manifest carries {} this binary does not know: {} — a newer yunta \
             wrote it, and what it recorded there is not read here",
            yunta_core::text::counted(open.manifest.unknown.len(), "key"),
            open.manifest.unknown_keys().join(", ")
        ));
    }
    let manifest = open.manifest.doc;

    let now = ctx.clock.now();
    if json {
        return crate::json::print_json(&crate::json::RunDocument::of(
            run_id, &events, &manifest, now,
        ));
    }

    let frame = progress::frame(run_id, &manifest, &events, now);
    let state = yunta_engine::derive(&events);
    println!("run {run_id}: {}", progress::summary(&frame));
    print_derived(&frame, &state);
    print_decision(run_id, &manifest, &events, &frame.phase);
    Ok(Outcome::Success)
}

/// The detail under the summary: every node the run's frozen workflow
/// declares and every task the log registered, what each failure names,
/// and what the run has spent.
///
/// The nodes come from the frame — declaration order, each `parallel`
/// group followed by its own children, the ones this mode leaves out
/// among them and labelled `skipped`. One derivation for the whole
/// page: the same list, in the same order, the live view draws.
fn print_derived(frame: &RunFrame, state: &yunta_engine::RunState) {
    if !frame.nodes.is_empty() {
        println!("nodes:");
        for node in &frame.nodes {
            // A group's children sit one step under it, exactly as the
            // live view and the chronicle place what belongs to a node.
            let under = match node.group {
                Some(_) => indent(1 + CHILD_DEPTH),
                None => INDENT.to_string(),
            };
            println!(
                "{under}{}: {}",
                node.id,
                NodeDisplay::standing(&node.state).label()
            );
        }
    }

    if !state.tasks.is_empty() {
        println!("tasks:");
        let mut tasks: Vec<_> = state.tasks.iter().collect();
        tasks.sort_by(|a, b| a.0.cmp(b.0));
        for (id, record) in tasks {
            println!("{INDENT}{id}: {}", task_status_label(record.status));
        }
    }

    print_failures(frame, state);

    println!(
        "tokens: {} in / {} out",
        state.total_tokens().input,
        state.total_tokens().output
    );
}

/// What a parked run is waiting on, printed last because it is what the
/// reader acts on next: the decision the log reconstructs, with the
/// command that answers it, or — for a pause that reconstructs none —
/// the sentence the frame carries for it and the way back into the run.
/// A run that is not parked prints nothing here.
fn print_decision(run_id: &RunId, manifest: &Manifest, events: &[StoredEvent], phase: &RunPhase) {
    let Some(waiting) = advice::parked(phase) else {
        return;
    };
    match yunta_engine::current_escalation(manifest, &yunta_engine::derive(events)) {
        Some((node, escalation)) => print!(
            "{}",
            decision::block(decision::Layout::Page, run_id, &node, &escalation)
        ),
        None => print!(
            "{}",
            decision::without_menu(run_id, &advice::parked_in_full(waiting))
        ),
    }
}

/// Every failure with more than one line of detail, laid out one block
/// per failing document: the path a reader opens, then that document's
/// own problems under it.
///
/// The node list above stays scannable at one line each, which means
/// collapsing them there. Doing only that would leave the one surface a
/// person opens to find out what went wrong unable to say. A node that
/// failed on two artifacts says which problem came from which, because
/// the log records each document's problems with the document.
fn print_failures(frame: &RunFrame, state: &yunta_engine::RunState) {
    let failed: Vec<(&NodeId, &Failure)> = frame
        .nodes
        .iter()
        .filter_map(|node| match state.nodes.state(&node.id) {
            Some(NodeState::Failed { failure, .. }) => Some((&node.id, failure)),
            _ => None,
        })
        .filter(|(_, failure)| match failure {
            Failure::Artifacts { artifacts } => !artifacts.is_empty(),
            Failure::Message { outcome } => outcome.contains('\n'),
        })
        .collect();
    if failed.is_empty() {
        return;
    }
    println!("failures:");
    // A document's problems hang under the node that named it, which is
    // itself one step under the heading.
    let detail = indent(2);
    for (id, failure) in failed {
        println!("{INDENT}{id}:");
        match failure {
            Failure::Artifacts { artifacts } => {
                for artifact in artifacts {
                    println!(
                        "{}",
                        yunta_core::text::indent(&artifact.to_string(), &detail)
                    );
                }
            }
            Failure::Message { outcome } => {
                println!("{}", yunta_core::text::indent(outcome, &detail));
            }
        }
    }
}

/// The event schema's snake_case task-status names — user output never
/// leaks Rust identifiers.
pub(crate) fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Ready => "ready",
        TaskStatus::Running => "running",
        TaskStatus::Done => "done",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
}
