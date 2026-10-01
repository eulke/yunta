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

use std::path::Path;

use yunta_core::events::{Failure, StoredEvent, TaskStatus};
use yunta_core::Clock;
use yunta_core::{Manifest, NodeId, RunId};
use yunta_engine::{NodeState, RunFrame, RunPhase};

use crate::commands::advice;
use crate::context::Context;
use crate::error::note;
use crate::error::{CliError, Outcome};
use crate::render::state::RunWord;
use crate::render::{indent, NodeDisplay, CHILD_DEPTH, INDENT};

pub async fn status(run_id: &RunId, json: bool) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let open = ctx.open_run(run_id).await?;
    let events = open.events;
    let run_dir = open.run_dir;
    let engine = yunta_engine::engine_liveness(&run_dir, &yunta_engine::lock::SystemProbe);
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
            run_id, &events, &manifest, now, engine,
        ));
    }

    let frame = progress::frame(run_id, &manifest, &events, now);
    let state = yunta_engine::derive(&events);
    println!("run {run_id}: {}", progress::summary(&frame, engine));
    print_derived(&frame, &state, &run_dir);
    print_stall(run_id, &frame, engine);
    let tree = ctx.project.run_tree(&manifest, run_id, &ctx.cwd);
    print_decision((run_id, &manifest, &tree), &events, &frame.phase);
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
fn print_derived(frame: &RunFrame, state: &yunta_engine::RunState, run_dir: &Path) {
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
            if let Some(failed) = state
                .nodes
                .get(&node.id)
                .and_then(|record| record.last_tool_failure.as_ref())
            {
                println!(
                    "{under}  last failed call of attempt: {} ({})",
                    failed.tool.name(),
                    failed.cause.as_str()
                );
            }
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

    print_failures(frame, state, run_dir);

    println!(
        "tokens: {} in / {} out",
        state.total_tokens().input,
        state.total_tokens().output
    );
    if let Some(drift) = state.run.environment_drift() {
        println!("environment: {drift}");
    }
    if let Some((times, slept)) = state.run.suspensions().summary() {
        println!(
            "host: suspended {} for {} in all — durations leave it out",
            yunta_core::text::counted(times, "time"),
            crate::render::format_duration(slept)
        );
    }
}

/// What a run whose engine is gone needs, printed where a parked run's
/// decision goes: nothing moves it again until a person hands it back
/// to an engine, or stops it where it is.
fn print_stall(run_id: &RunId, frame: &RunFrame, engine: yunta_engine::EngineLiveness) {
    if RunWord::observed(frame, engine) != RunWord::Stalled {
        return;
    }
    println!("no process is driving this run: the engine that ran it is gone");
    println!(
        "{INDENT}{}   continues it from its log",
        advice::resume(run_id)
    );
    println!("{INDENT}{}   closes it for good", advice::close(run_id));
}

/// What a parked run is waiting on, printed last because it is what the
/// reader acts on next: the decision the log reconstructs, with the
/// command that answers it, or — for a pause that reconstructs none —
/// the sentence the frame carries for it and the way back into the run.
/// A run that is not parked prints nothing here.
fn print_decision(
    (run_id, manifest, tree): (&RunId, &Manifest, &Path),
    events: &[StoredEvent],
    phase: &RunPhase,
) {
    let Some(waiting) = advice::parked(phase) else {
        return;
    };
    match yunta_engine::current_escalation(manifest, &yunta_engine::derive(events)) {
        Some((node, escalation)) => {
            // Where a person acts before answering, said once above the
            // page and whole, so it can be copied into another terminal.
            println!("{}", decision::run_tree_line(tree));
            print!(
                "{}",
                decision::block(decision::Layout::Page, run_id, &node, &escalation)
            );
        }
        None => print!(
            "{}",
            decision::without_menu(run_id, &advice::parked_in_full(waiting))
        ),
    }
    println!("{INDENT}{}   closes it for good", advice::close(run_id));
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
fn print_failures(frame: &RunFrame, state: &yunta_engine::RunState, run_dir: &Path) {
    let failed: Vec<(&NodeId, &Failure)> = frame
        .nodes
        .iter()
        .filter_map(|node| match state.nodes.state(&node.id) {
            Some(NodeState::Failed { failure, .. }) => Some((&node.id, failure)),
            _ => None,
        })
        .filter(|(_, failure)| has_detail(failure))
        .collect();
    if failed.is_empty() {
        return;
    }
    println!("failures:");
    for (id, failure) in failed {
        println!("{INDENT}{id}:");
        print_detail(failure, run_dir);
    }
}

/// Whether `failure` says more than the node's own line has room for.
fn has_detail(failure: &Failure) -> bool {
    match failure {
        Failure::Artifacts { artifacts } => !artifacts.is_empty(),
        // A death with nothing to show fits in the node's own line;
        // the lines the CLI left behind are what needs the room.
        Failure::SessionDied { died } => died
            .exit
            .as_ref()
            .is_some_and(|exit| !exit.stderr_tail.is_empty()),
        // One path fits in the node's own line; a list reads better a
        // path to a line.
        Failure::ScopeViolated { outside_scope } => outside_scope.len() > 1,
        Failure::PathsDenied { denied_paths } => denied_paths.len() > 1,
        // What was asked for and why fit in the node's own line, and so
        // does the key a config leaves unset.
        Failure::ScopeRequested { .. } | Failure::Unset { .. } => false,
        Failure::Message { outcome } => outcome.contains('\n'),
        // What a failing command printed, and where the rest of it is,
        // is the reason a person reads; the node's own line has room
        // for neither.
        Failure::Exited { exited } => !exited.tail.is_empty() || exited.output.is_some(),
        // Why it was not run again, then what the attempt that ran
        // failed with: more than a line holds.
        Failure::Unchanged { .. } => true,
    }
}

/// The detail of one failure, hanging under the node that failed, which
/// is itself one step under the heading.
fn print_detail(failure: &Failure, run_dir: &Path) {
    let detail = indent(2);
    match failure {
        Failure::Artifacts { artifacts } => {
            for artifact in artifacts {
                println!(
                    "{}",
                    yunta_core::text::indent(&artifact.to_string(), &detail)
                );
            }
        }
        // How the process went, then what it said on its way out: the
        // last line is already in the node's own line, and the ones
        // above it are what a person reads to know why.
        Failure::SessionDied { died } => {
            println!("{}", yunta_core::text::indent(&died.to_string(), &detail));
            for line in died.exit.iter().flat_map(|exit| &exit.stderr_tail) {
                println!("{}", yunta_core::text::indent(line, &indent(3)));
            }
        }
        // How the command ended, then what it printed last, then where
        // everything it printed is kept.
        Failure::Exited { exited } => {
            println!("{detail}{}", exited.headline());
            for line in &exited.tail {
                println!("{}{line}", indent(3));
            }
            if let Some(output) = &exited.output {
                println!(
                    "{detail}whole output: {}",
                    yunta_engine::ObjectStore::at(run_dir)
                        .path_of(output)
                        .display()
                );
            }
        }
        Failure::ScopeViolated { outside_scope } => {
            println!("{detail}outside the declared globs:");
            for path in outside_scope {
                println!("{}{}", indent(3), path.display());
            }
        }
        Failure::PathsDenied { denied_paths } => {
            println!(
                "{detail}denied to every session of the run — by the project \
                 (permissions.paths.deny), or as a test a person approved:"
            );
            for path in denied_paths {
                println!("{}{}", indent(3), path.display());
            }
        }
        Failure::ScopeRequested { .. }
        | Failure::Unset { .. }
        | Failure::Unchanged { .. }
        | Failure::Message { .. } => {
            println!(
                "{}",
                yunta_core::text::indent(&failure.to_string(), &detail)
            );
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
