//! What a surface says beside a run that has stopped: what the run is
//! waiting on, and the steps a person takes to move it.
//!
//! Every one of them is printed, at some point, beside a run a person
//! still has to act on: in the live region's attention row, in the block
//! that closes a run out, in `yunta status` and in the JSON a program
//! reads. They are said once here so a reader who met a phrase on one
//! surface meets the same phrase on the next. Each step is a `NextStep`,
//! written as a command line by `yunta_render::command`, the run named the
//! way its reader copies it.

use surco::words::Standing;
use yunta_core::events::{ArtifactId, GateWaitingPayload, NodeWait, PauseReason, Stuck};
use yunta_core::{ArtifactKind, RunId};
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};

use yunta_core::step::NextStep;
use yunta_render::command::{self, Reader};

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
    let run = &frame.run_id;
    let said = |step: NextStep, does| (command::line_for(&step, reader), does);
    let stops = || {
        said(
            NextStep::Cancel { run: run.clone() },
            "stops it and everything under it",
        )
    };
    let open = matches!(
        frame.phase,
        RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
    );
    match frame.standing {
        Standing::Stalled => goes_on(run, reader, "continues it from its log"),
        Standing::Waiting => goes_on(run, reader, "waits for the reset, then drives on"),
        // Asked live, the question is answered where it is asked: what a
        // command here can do is stop the run.
        Standing::NeedsYou if matches!(frame.phase, RunPhase::Created | RunPhase::Running) => {
            vec![stops()]
        }
        Standing::NeedsYou => after_needs_you(frame, reader, decision),
        Standing::Finished | Standing::Reported => {
            vec![said(
                NextStep::Receipt { run: run.clone() },
                "certifies what it did",
            )]
        }
        Standing::Broken => {
            vec![said(
                NextStep::Verify { run: run.clone() },
                "says where its log stops reading",
            )]
        }
        Standing::Created | Standing::Running => vec![stops()],
        Standing::Stopped if open => goes_on(run, reader, "continues it where it stopped"),
        Standing::Failed
        | Standing::Stopped
        | Standing::Promoted
        | Standing::Rerouted
        | Standing::Skipped
        | Standing::NeverRan => Vec::new(),
    }
}

/// The two ways a stopped run goes on: handed back to the engine, its resume
/// doing what `resumes` says, or closed for good.
fn goes_on(run: &RunId, reader: Reader, resumes: &'static str) -> Vec<(String, &'static str)> {
    vec![
        (
            command::line_for(&NextStep::Resume { run: run.clone() }, reader),
            resumes,
        ),
        (
            command::line_for(&NextStep::Close { run: run.clone() }, reader),
            "closes it for good",
        ),
    ]
}

/// The child a parked run waits on. What it waits on stands on the child's log, so no
/// answer given to the parent can reach it, and the way there is what a reader needs first.
fn child_parked_on(frame: &RunFrame) -> Option<&RunId> {
    match parked(&frame.phase)?.reason()? {
        PauseReason::ChildPaused { run, .. } => Some(run),
        _ => None,
    }
}

/// What a person types next for a run that needs them. A run parked on a
/// child is offered the way to that child first. With the decision it stopped
/// on, a retry is offered when that failure waits for the run's tree to
/// change; either way the run can be closed for good.
fn after_needs_you(
    frame: &RunFrame,
    reader: Reader,
    decision: Option<&GateWaitingPayload>,
) -> Vec<(String, &'static str)> {
    let run = &frame.run_id;
    let said = |step: NextStep, does| (command::line_for(&step, reader), does);
    let reaches = child_parked_on(frame)
        .map(|child| said(NextStep::status(child), "shows what it is waiting on"));
    let Some(decision) = decision else {
        return reaches
            .into_iter()
            .chain(goes_on(run, reader, "hands it back once that is settled"))
            .collect();
    };
    let mut steps: Vec<_> = reaches.into_iter().collect();
    if shows_plan(decision) {
        steps.push(said(
            NextStep::Review { run: run.clone() },
            "reads its plan, and answers it there",
        ));
    }
    if waits_for_tree(decision) {
        steps.push(said(
            NextStep::Resume { run: run.clone() },
            "offers a retry once the run's tree changed",
        ));
    }
    steps.push(said(
        NextStep::Close { run: run.clone() },
        "closes it for good",
    ));
    steps
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
