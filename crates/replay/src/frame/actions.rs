//! What a reader can do next for a run: each action is a step and what it does
//! to the run, chosen by where the run stands and the decision it stopped on.
//!
//! Data, so every surface offers the same: the CLI writes each step as the line
//! a reader types beside its effect's words, a terminal binds one to a key and a
//! desktop draws it as a button. The words and the spelling are
//! `yunta_render::command`'s; what a run offers is decided here, once.

use surco::words::Standing;
use yunta_core::events::{ArtifactId, GateWaitingPayload, PauseReason, Stuck};
use yunta_core::step::NextStep;
use yunta_core::{ArtifactKind, RunId};

use super::{RunFrame, RunPhase};

/// What a reader can do next for a run: the step, and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub step: NextStep,
    pub effect: Effect,
}

/// What an action does to the run it names. Its words are the reader's, in
/// `yunta_render::command`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    ContinuesFromLog,
    ContinuesWhereStopped,
    WaitsForReset,
    HandsBackOnceSettled,
    RetriesOnceTreeChanged,
    ReadsPlan,
    ShowsWhatItWaitsOn,
    Stops,
    Closes,
    Certifies,
    FindsWhereLogStops,
}

impl RunFrame {
    /// What a reader can do next for this run, by the word its standing
    /// reads. `decision` is the decision a run that needs a person stopped on,
    /// when its log rebuilds one: a plan it shows is read and answered at a
    /// terminal, and a failure no retry can change until the run's tree moves
    /// is offered the resume that retries once it did. A run parked on a child
    /// is offered the way to that child first.
    pub fn actions(&self, decision: Option<&GateWaitingPayload>) -> Vec<Action> {
        let run = &self.run_id;
        let stops = || act(NextStep::Cancel { run: run.clone() }, Effect::Stops);
        let open = matches!(
            self.phase,
            RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. }
        );
        match self.standing {
            Standing::Stalled => goes_on(run, Effect::ContinuesFromLog),
            Standing::Waiting => goes_on(run, Effect::WaitsForReset),
            // Asked live, the question is answered where it is asked: what a
            // command here can do is stop the run.
            Standing::NeedsYou if matches!(self.phase, RunPhase::Created | RunPhase::Running) => {
                vec![stops()]
            }
            Standing::NeedsYou => self.needs_you(decision),
            Standing::Finished | Standing::Reported => {
                vec![act(
                    NextStep::Receipt { run: run.clone() },
                    Effect::Certifies,
                )]
            }
            Standing::Broken => {
                vec![act(
                    NextStep::Verify { run: run.clone() },
                    Effect::FindsWhereLogStops,
                )]
            }
            Standing::Created | Standing::Running => vec![stops()],
            Standing::Stopped if open => goes_on(run, Effect::ContinuesWhereStopped),
            Standing::Failed
            | Standing::Stopped
            | Standing::Promoted
            | Standing::Rerouted
            | Standing::Skipped
            | Standing::NeverRan => Vec::new(),
        }
    }

    /// What a run that needs a person offers: the way to a child it is parked
    /// on, then, with the decision it stopped on, its plan to read and a retry
    /// once the tree changed where the failure waits for that; either way the
    /// run can be closed for good.
    fn needs_you(&self, decision: Option<&GateWaitingPayload>) -> Vec<Action> {
        let run = &self.run_id;
        let reaches = child_parked_on(self)
            .map(|child| act(NextStep::status(child), Effect::ShowsWhatItWaitsOn));
        let Some(decision) = decision else {
            return reaches
                .into_iter()
                .chain(goes_on(run, Effect::HandsBackOnceSettled))
                .collect();
        };
        let mut actions: Vec<Action> = reaches.into_iter().collect();
        if shows_plan(decision) {
            actions.push(act(
                NextStep::Review { run: run.clone() },
                Effect::ReadsPlan,
            ));
        }
        if waits_for_tree(decision) {
            actions.push(act(
                NextStep::Resume { run: run.clone() },
                Effect::RetriesOnceTreeChanged,
            ));
        }
        actions.push(act(NextStep::Close { run: run.clone() }, Effect::Closes));
        actions
    }
}

/// `step`, offered for what `effect` says it does.
fn act(step: NextStep, effect: Effect) -> Action {
    Action { step, effect }
}

/// The two ways a stopped run goes on: handed back to the engine, its resume
/// doing what `resumes` says, or closed for good.
fn goes_on(run: &RunId, resumes: Effect) -> Vec<Action> {
    vec![
        act(NextStep::Resume { run: run.clone() }, resumes),
        act(NextStep::Close { run: run.clone() }, Effect::Closes),
    ]
}

/// The child a parked run waits on. What it waits on stands on the child's log, so no
/// answer given to the parent can reach it, and the way there is what a reader needs first.
fn child_parked_on(frame: &RunFrame) -> Option<&RunId> {
    match frame.phase.waiting_on()?.reason()? {
        PauseReason::ChildPaused { run, .. } => Some(run),
        _ => None,
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

/// Whether the failure a decision is about offers no retry until the run's
/// tree changes. The same check on the same tree answers the same, so the
/// menu says that much and the way back is a resume once the tree moved.
fn waits_for_tree(decision: &GateWaitingPayload) -> bool {
    decision.failure().and_then(|menu| menu.stuck.as_ref()) == Some(&Stuck::TreeUnchanged)
}
