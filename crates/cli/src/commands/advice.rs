//! What a surface says beside a run that has stopped: what the run is
//! waiting on, and the commands a person runs to move it.
//!
//! Every one of them is printed, at some point, beside a run a person
//! still has to act on: in the live region's attention row, in the block
//! that closes a run out, in `yunta status`, in the JSON a program
//! reads, and in the diagnostic a command hands back when it could not
//! finish its own job. They are spelled once here so a reader who met a
//! phrase on one surface meets the same phrase on the next, and so
//! renaming a subcommand is a change to this file rather than a hunt
//! through prose.
//!
//! Each command carries what the run is called by — its handle on a
//! line a person reads, its whole id in a document a program reads —
//! and nothing else a reader would have to invent — a gate's option stays `<option>`, because an option
//! id printed as an example gets pasted, and no id printed beside a run
//! is ever one that run's own menu does not offer.

use surco::words::Standing;
use yunta_core::events::{ArtifactId, GateWaitingPayload, NodeWait, PauseReason, Stuck};
use yunta_core::step::NextStep;
use yunta_core::{ArtifactKind, RunId};
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};

use crate::error::Reader;

/// What holds a run whose log says it is moving and whose engine is gone.
pub(crate) const STALLED: &str = "no process is driving it: the engine that ran it is gone";

/// What holds a run whose engine is asking a person at its terminal: a
/// question asked live is on no log until it is answered, so the only
/// place to answer it is where it is asked. With `at`, the sentence says
/// how long it has been asking; a page whose headline says it leaves it
/// out.
pub(crate) fn asking(
    prompt: &yunta_replay::registry::Prompt,
    at: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    let who = prompt
        .node
        .as_ref()
        .map_or_else(|| "the run".to_string(), |node| format!("node `{node}`"));
    let waited = at.map(|at| {
        let waited = (at - prompt.since).to_std().unwrap_or_default();
        format!(" for {}", surco::figures::duration(waited))
    });
    format!(
        "{who} is asking at its terminal (pid {}){}; answer it there",
        prompt.pid,
        waited.unwrap_or_default()
    )
}

/// Whom a live engine is asking, on the one line a row has room for: the
/// node, and that the answer is given at its terminal.
pub(crate) fn asking_on(prompt: &yunta_replay::registry::Prompt) -> String {
    match &prompt.node {
        Some(node) => format!("node `{node}` asks at its terminal"),
        None => "the run asks at its terminal".to_string(),
    }
}

/// What a run is parked on, or `None` for a run nobody has to touch —
/// the one answer that decides whether a surface says anything about a
/// decision at all.
pub(crate) fn parked(phase: &RunPhase) -> Option<&WaitingOn> {
    match phase {
        RunPhase::Waiting { on } => Some(on),
        RunPhase::Created
        | RunPhase::Running
        | RunPhase::Finished
        | RunPhase::Failed { .. }
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. }
        | RunPhase::Broken { .. } => None,
    }
}

/// What a parked run is waiting on, on the one line a row has room for:
/// the node, and the forge handle it was published under when it has
/// one. A row in a listing carries one of these per run, so it names the
/// node and stops there.
///
/// A pause the log recorded under no node has no node to name, and says
/// its own reason instead, collapsed onto that line.
pub(crate) fn parked_on(on: &WaitingOn) -> String {
    match on {
        WaitingOn::Node { node, on, .. } => match on {
            NodeWait::Gate {
                external_ref: Some(handle),
            } => format!("node `{node}`, published at {handle}"),
            NodeWait::Gate { external_ref: None } | NodeWait::Questions { .. } => {
                format!("node `{node}`")
            }
        },
        WaitingOn::Run { reason } => surco::text::one_line(&yunta_render::pause::sentence(reason)),
    }
}

/// What a parked run is waiting on, as a page that quotes the record
/// beside it says it: an escalation's claim alone — the page quotes its
/// record as a failed node's evidence or in the decision's account — and
/// the sentence the engine recorded for any other pause.
pub(crate) fn parked_claim(on: &WaitingOn) -> String {
    let escalation = match on {
        WaitingOn::Node {
            reason: Some(PauseReason::Escalation(escalation)),
            ..
        }
        | WaitingOn::Run {
            reason: PauseReason::Escalation(escalation),
        } => escalation,
        WaitingOn::Node { .. } | WaitingOn::Run { .. } => return parked_in_full(on),
    };
    crate::commands::status::decision::claim_words(&(**escalation).clone().into_payload())
}

/// What a parked run is waiting on, at the length a page has room for:
/// the sentence the engine recorded when it stopped the run, which names
/// what a node id cannot — which questions are still unanswered, which
/// cap a budget hit, which review a forge is holding.
///
/// A node parked while the run itself keeps moving has no such sentence
/// on the log, and reads as [`parked_on`].
pub(crate) fn parked_in_full(on: &WaitingOn) -> String {
    match on {
        WaitingOn::Node {
            reason: Some(reason),
            ..
        }
        | WaitingOn::Run { reason } => {
            surco::text::one_line(&yunta_render::pause::sentence(reason))
        }
        WaitingOn::Node { reason: None, .. } => parked_on(on),
    }
}

/// Answers a run parked on a decision, with the option left for the
/// reader to pick off the menu printed above it.
pub(crate) fn resolve_gate(run: &str) -> String {
    format!("yunta resolve-gate {run} <option>")
}

/// Puts a run's decision to a person: on a terminal, its documents and
/// its menu; off one, a command per option.
pub(crate) fn decide(run: &str) -> String {
    format!("yunta resolve-gate {run}")
}

/// Reads the plan a parked run waits on, and answers it at a terminal.
pub(crate) fn review(run: &str) -> String {
    format!("yunta review {run}")
}

/// Hands a run back to the engine, once whatever stopped it is settled
/// somewhere else — a scope, an answers file, a review on a forge, a
/// child's own decision.
pub(crate) fn resume(run: &str) -> String {
    format!("yunta resume {run}")
}

/// A separate resume, named in a sentence about the command and no run of its own.
pub(crate) fn resume_any() -> String {
    "yunta resume".to_string()
}

/// Stops a run and the whole process tree under it.
pub(crate) fn cancel(run: &str) -> String {
    format!("yunta cancel {run}")
}

/// Closes a stopped run nobody is going to continue.
pub(crate) fn close(run: &str) -> String {
    format!("yunta close {run}")
}

/// The command that draws a run's page again and opens it.
pub(crate) fn open(run: &str) -> String {
    format!("yunta open {run}")
}

/// Reclaims the disk runs past their retention hold.
pub(crate) fn gc() -> String {
    "yunta gc".to_string()
}

/// Lists the documents `yunta schema` shows.
pub(crate) fn schemas() -> String {
    "yunta schema".to_string()
}

/// Shows the shape of `document`, a name `yunta schema` lists: as a person
/// reads it, or with `json` as the JSON Schema an editor validates against.
pub(crate) fn schema(document: &str, json: bool) -> String {
    match json {
        true => format!("{} {document} --json", schemas()),
        false => format!("{} {document}", schemas()),
    }
}

/// The command that lists every key a workflow and each kind of node read.
pub(crate) fn schema_workflow() -> String {
    schema(super::schema::Shown::Workflow.as_str(), false)
}

/// Runs a project's test cases.
pub(crate) fn test() -> String {
    "yunta test".to_string()
}

/// Says whether this machine is ready to run.
pub(crate) fn doctor() -> String {
    "yunta doctor".to_string()
}

/// Checks a workflow, named by what it is called in a sentence.
pub(crate) fn check() -> String {
    "yunta check".to_string()
}

/// What a run's numbers are read with.
pub(crate) fn stats() -> String {
    "yunta stats".to_string()
}

/// Prepares a repository for yunta.
pub(crate) fn init() -> String {
    "yunta init".to_string()
}

/// `yunta pack <verb>`, for `add`, `update`, `remove` or `new`.
pub(crate) fn pack(verb: &str) -> String {
    format!("yunta pack {verb}")
}

/// Lists the workflows this repository can run.
pub(crate) fn list() -> String {
    "yunta list".to_string()
}

/// Lists this repository's runs, what needs you first.
pub(crate) fn list_runs() -> String {
    "yunta list --runs".to_string()
}

/// Lists every run on this machine.
pub(crate) fn list_every_run() -> String {
    format!("{} --all", list_runs())
}

/// Lists this repository's runs, the closed ones too.
pub(crate) fn list_closed_runs() -> String {
    format!("{} --full", list_runs())
}

/// What yunta does, and every command it takes.
pub(crate) fn help() -> String {
    "yunta --help".to_string()
}

/// Starts a workflow, named the way the catalog names it.
pub(crate) fn run(workflow: &str) -> String {
    format!("yunta run {workflow}")
}

/// Shows where a run stands, derived from its own log.
pub(crate) fn status(run: &str) -> String {
    format!("yunta status {run}")
}

/// Follows a run while it moves, from any process: what `yunta run` draws,
/// read from the run's log.
pub(crate) fn follow(run: &str) -> String {
    format!("{} --follow", status(run))
}

/// Shows one node of a run whole: its failure, the end of what it
/// printed and where all of it is.
pub(crate) fn status_node(run: &str, node: &str) -> String {
    format!("yunta status {run} --node {node}")
}

/// Shows a commit a run's work is held in, by the id a reader meets it under: a blocked
/// task's last attempt left its work in one.
pub(crate) fn show_commit(commit: &str) -> String {
    format!("git show {commit}")
}

/// Gathers a finished run's manifest, log and artifacts into the bundle
/// that certifies it.
pub(crate) fn receipt(run: &str) -> String {
    format!("yunta receipt {run}")
}

/// Walks a run's event hash chain, which is what says where a log stopped
/// being readable.
pub(crate) fn verify(run: &str) -> String {
    format!("yunta verify {run}")
}

/// What a person types next for a run its word describes, each command
/// with what it does. `decision` is the decision a run that needs the
/// person stopped on, when the log rebuilds one: its options carry their
/// own commands, and a failure that no retry can change until the tree
/// moves says so beside the command that hands the run back. A run parked on
/// a child is offered the way to that child first.
pub(crate) fn after(
    frame: &RunFrame,
    decision: Option<&GateWaitingPayload>,
) -> Vec<(String, &'static str)> {
    after_for(frame, decision, Reader::Person)
}

/// What `reader` types next for a run its word describes: the steps
/// [`after`] gives a person, with the run named the way `reader` copies it.
pub(crate) fn after_for(
    frame: &RunFrame,
    decision: Option<&GateWaitingPayload>,
    reader: Reader,
) -> Vec<(String, &'static str)> {
    let run = named(&frame.run_id, reader);
    let open = matches!(
        frame.phase,
        RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
    );
    match frame.standing {
        Standing::Stalled => vec![
            (resume(run), "continues it from its log"),
            (close(run), "closes it for good"),
        ],
        Standing::Waiting => after_reset(run),
        // Asked live, the question is answered where it is asked: what a
        // command here can do is stop the run.
        Standing::NeedsYou if matches!(frame.phase, RunPhase::Created | RunPhase::Running) => {
            vec![(cancel(run), "stops it and everything under it")]
        }
        Standing::NeedsYou => {
            let reaches = child_parked_on(frame)
                .map(|child| (status(named(child, reader)), "shows what it is waiting on"));
            reaches
                .into_iter()
                .chain(after_needs_you(run, decision))
                .collect()
        }
        Standing::Finished | Standing::Reported => vec![(receipt(run), "certifies what it did")],
        Standing::Broken => vec![(verify(run), "says where its log stops reading")],
        Standing::Created | Standing::Running => {
            vec![(cancel(run), "stops it and everything under it")]
        }
        Standing::Stopped if open => vec![
            (resume(run), "continues it where it stopped"),
            (close(run), "closes it for good"),
        ],
        Standing::Failed
        | Standing::Stopped
        | Standing::Promoted
        | Standing::Rerouted
        | Standing::Skipped
        | Standing::NeverRan => Vec::new(),
    }
}

/// The child a parked run waits on. What it waits on stands on the child's log, so no
/// answer given to the parent can reach it, and the way there is what a reader needs first.
fn child_parked_on(frame: &RunFrame) -> Option<&RunId> {
    match parked(&frame.phase)?.reason()? {
        PauseReason::ChildPaused { run, .. } => Some(run),
        _ => None,
    }
}

/// What a person types next for a run that needs them. With the decision it
/// stopped on, a retry is offered when that failure waits for the run's tree
/// to change; either way the run can be closed for good.
fn after_needs_you(
    run: &str,
    decision: Option<&GateWaitingPayload>,
) -> Vec<(String, &'static str)> {
    match decision {
        None => vec![
            (resume(run), "hands it back once that is settled"),
            (close(run), "closes it for good"),
        ],
        Some(decision) => {
            let mut steps = Vec::new();
            if shows_plan(decision) {
                steps.push((review(run), "reads its plan, and answers it there"));
            }
            if waits_for_tree(decision) {
                steps.push((resume(run), "offers a retry once the run's tree changed"));
            }
            steps.push((close(run), "closes it for good"));
            steps
        }
    }
}

/// Whether a decision shows a plan, the tasks document a person reads and
/// answers at a terminal.
fn shows_plan(decision: &GateWaitingPayload) -> bool {
    decision.shows().iter().any(|shown| {
        matches!(
            shown.artifact,
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks
            }
        )
    })
}

/// What a person types next for a run paused on a limit's reset. The resume
/// that hands it back waits for the reset before it takes anything, so the
/// words say when it moves rather than asking a person to settle something
/// first.
pub(crate) fn after_reset(run: &str) -> Vec<(String, &'static str)> {
    vec![
        (resume(run), "waits for the reset, then drives on"),
        (close(run), "closes it for good"),
    ]
}

/// What a person or an agent types next, in the words of the command that
/// accepts it. `reader` decides how the run is named: a person reads its
/// handle, and an agent copies its whole id.
pub(crate) fn spell_for(step: &NextStep, reader: Reader) -> String {
    match step {
        NextStep::Status { run, node: None } => status(named(run, reader)),
        NextStep::Status {
            run,
            node: Some(node),
        } => status_node(named(run, reader), node.as_str()),
        NextStep::Resume { run } => resume(named(run, reader)),
        NextStep::Cancel { run } => cancel(named(run, reader)),
        NextStep::Close { run } => close(named(run, reader)),
        NextStep::Receipt { run } => receipt(named(run, reader)),
        NextStep::Verify { run } => verify(named(run, reader)),
        NextStep::ResolveGate {
            run, option: None, ..
        } => decide(named(run, reader)),
        NextStep::ResolveGate {
            run,
            option: Some(option),
            text,
        } => {
            let answer = if *text { " --text \"<answer>\"" } else { "" };
            format!("yunta resolve-gate {} {option}{answer}", named(run, reader))
        }
        NextStep::Gc => gc(),
        NextStep::Check { workflow } => format!("yunta check {workflow}"),
    }
}

/// What a person types next: the run named by its handle, the way every
/// line a person reads names it.
pub(crate) fn spell(step: &NextStep) -> String {
    spell_for(step, Reader::Person)
}

/// The run as `reader` names it on a line: its handle for a person, its
/// whole id for an agent.
pub(crate) fn named(run: &RunId, reader: Reader) -> &str {
    match reader {
        Reader::Person => run.handle(),
        Reader::Agent => run.as_str(),
    }
}

/// Whether the failure a decision is about offers no retry until the run's
/// tree changes. The same check on the same tree answers the same, so the
/// menu says that much and the way back is a resume once the tree moved.
fn waits_for_tree(decision: &GateWaitingPayload) -> bool {
    decision.failure().and_then(|menu| menu.stuck.as_ref()) == Some(&Stuck::TreeUnchanged)
}

/// The node a run paused on a limit's reset is held for, and the moment
/// the run resumes at by itself. `None` for a run in any other state.
pub(crate) fn reset_pause(
    phase: &RunPhase,
) -> Option<(&yunta_core::NodeId, chrono::DateTime<chrono::Utc>)> {
    match phase {
        RunPhase::Waiting {
            on: WaitingOn::Run { reason },
        } => reason.reset_wait(),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
