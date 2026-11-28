//! See [`super`]. What a run's screen stands as in its main area: one [`Scene`], made by
//! one function of the run in view, of what it waits on a person for as read for the
//! pause it stands at, of the node pointed at, of whether the run leads and of whether the
//! plan its decision is on stands in front of its card.
//!
//! While the run leads, its phase says which scene stands. A run that moves stands as the
//! node pointed at stands at work (a loop's board, the findings of a node that posted or
//! answered one, else its work), by the word its frame reads: still with no process
//! driving it, asked elsewhere while its process asks at its own terminal. A run parked
//! stands as what its pause asks, once read for that pause: the round, or the decision as
//! the menu of the escalation the engine rebuilt says, never the log's: a desk for a gate
//! that shows a plan where the change it counted holds no file, so the plan leads, or where
//! the plan stands in front, read from its card or opened on as `review` opens it; a card
//! otherwise, resting on what that menu and its failure's cause name ([`Grounds`]); as its
//! pull request; or still where nothing here answers it ([`Standstill`]: a pause no option
//! answers here, no room, nodes left half done, a stop). A run that ended stands as its
//! receipt or its end. Work that waits on a clock, a pause whose ask is not read yet or
//! could not be read, a node waiting with no pause standing and an answer awaiting the
//! resume that acts on it stand as the node pointed at stands at work, still. A prompt
//! stands only while the run leads: while it does not, the node pointed at stands as it
//! does at work while it works; once it ended, as the findings scene where it posted or
//! answered one and the run waits on no person at it; as its own scene otherwise. On a run
//! that ended otherwise than finished, which holds no prompt, the node the run stands on at
//! its end shows the end whoever points at it.
//!
//! Every closed set it reads is matched by name with no wildcard arm: `RunPhase`, the
//! run's and the node's `Standing`, `WaitingOn`, `PauseReason`, `Menu`, `NodeKind` and
//! `Failure`. A variant added to any of them does not compile here until it has a scene.
//! The module denies `clippy::wildcard_enum_match_arm`, and `tests.rs` makes a scene for
//! every `Standing` of the node pointed at; no `if let` or `matches!` decides a scene, a
//! `match` does. What already decides part of it is taken as given: whether the engine
//! rebuilds a decision (`awaited_escalation`, kept as the run in view's decision and read
//! as its `menu`), what the
//! run in view keeps as read for the pause it stands at (the decision or the round it
//! waits on, the change a gate counted), a failure's own questions, where the run puts its
//! pointer at its end (`lead`), and the plan a decision shows (`desk::plan_among`).
//!
//! The stage draws by scene (`pane`), reading what each shows and offers (`shows`): every
//! scene composes its pane ([`ComposedPane`], `pane`), the work, board, findings, a node's
//! own, a standstill's, a pull request's, a run asked elsewhere's, the receipt's, an ended
//! run's, a card on whatever it rests on, a round and the plan's desk. A round takes the place
//! of the run's nodes too, and the desk heads itself in the place of what stands across the
//! run, its nodes and what it spent.
//! The receipt names itself on the trail, where every other scene's trail names the node
//! pointed at. A scene says which of the acts the run offers its legend names
//! ([`NamedActs`]): none on a node's own, which asks nothing, what the run waits on being one
//! `a` or `f` away; going on alone where the run stands still for room, which `u` makes, or
//! on what is left half done.

#![deny(clippy::wildcard_enum_match_arm)]

mod grounds;
mod shows;

pub use grounds::Grounds;
pub(crate) use shows::{ComposedPane, NamedActs};

use chrono::{DateTime, Utc};
use surco::words::Standing;
use yunta_api::WaitingDecision;
use yunta_core::events::findings::FindingLedger;
use yunta_core::events::{Menu, PauseReason};
use yunta_core::{NodeId, NodeKind};
use yunta_render::page::desk;
use yunta_replay::frame::{NodeFrame, RunFrame, RunPhase, WaitingOn};

use super::{lead, Framed, RunStage};
use crate::{FollowedRuns, RunInView};
use grounds::rests_on;

/// What a run's screen stands as in its main area, for the node pointed at and whether the
/// run leads: one arm a layout, what differs inside it its data. Not a `Stage`, where a
/// person is in the application; not a `Moment`, one thing the log says happened.
#[derive(Debug, Clone, Copy)]
pub enum Scene<'r> {
    /// A node at work: what it hands over and says, or what it runs; and such a node's
    /// work standing still, held for a reset, waiting on what is not read yet or on its
    /// resume.
    Work,
    /// A loop at work, or standing still as work does: each task at work, how it is
    /// proved, its scope.
    Board,
    /// Findings posted, and the node reviews or fixes them.
    Findings,
    /// A round of questions the run waits on a person to answer.
    Round,
    /// A gate that shows a plan: the plan's desk, the gate's options at its foot.
    Desk,
    /// The decision a run waits on, as a card at the pane's foot over what it rests on.
    Card(&'r WaitingDecision, Grounds),
    /// A gate a forge holds: its pull request and how it stands.
    PullRequest,
    /// A run standing with nothing to answer here, and why.
    Standstill(Standstill),
    /// A question open at the terminal of the process that drives the run.
    AskedElsewhere,
    /// A run finished, reported or not: its receipt.
    Receipt,
    /// A run ended otherwise: failed, closed after a stop, promoted or broken; and the
    /// node it stands on at its end, whoever points at it.
    Ended,
    /// The node pointed at while the run does not lead, as it stands.
    Node,
}

/// Why a run stands with nothing to answer here: the pause it is parked on, by what moves
/// it again, or no process driving it. What its pane says and which of the run's acts its
/// legend names follow from it. Not `Standing::Stopped`, the word a run a person stopped
/// is called by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standstill {
    /// A pause no option here answers: a decision the engine rebuilds no menu for, or the
    /// pause of a child it runs. It is settled where it was raised, then handed back.
    Unanswered,
    /// No room on the disk for it to go on: room made, it is handed back.
    NoRoom,
    /// Nodes left half done, whose end the engine could not tell or that hold the rest:
    /// checked, it is handed back.
    HalfDone,
    /// A stop: a person's, before or after its engine died, a gate a person paused, or one
    /// a log recorded in a sentence. Not `Standing::Stopped`, which a recorded sentence
    /// is not called by.
    Stopped,
    /// Its log says it moves, and no process drives it.
    NoProcess,
}

impl RunStage {
    /// The scene the run `followed` keeps stands as at `now` on this stage: none until the
    /// run is read, nor for a run with no node.
    pub(crate) fn scene<'a>(
        &self,
        followed: &'a FollowedRuns,
        now: DateTime<Utc>,
    ) -> Option<Scene<'a>> {
        self.framed(followed, now)
            .and_then(|framed| self.scene_of(&framed))
    }

    /// The scene the run `framed` stands as on this stage, for the node pointed at.
    pub(super) fn scene_of<'a>(&self, framed: &Framed<'a>) -> Option<Scene<'a>> {
        let pointed = self.pointed_in(framed)?;
        let node = framed.frame.nodes.iter().find(|node| node.id == pointed)?;
        let plan = match self.plan_in_front(framed) {
            true => Plan::InFront,
            false => Plan::Behind,
        };
        Some(scene(
            framed.in_view,
            framed.frame,
            node,
            self.leads(),
            plan,
        ))
    }
}

/// Where the plan a decision is on stands, for a scene that shows it: in front of its card,
/// read from it or opened on, or behind it. Not whether the decision is on a plan, which
/// what it shows says.
#[derive(Debug, Clone, Copy)]
enum Plan {
    InFront,
    Behind,
}

/// The scene the run `run`, framed as `frame`, stands as for its node `pointed`: by the
/// run while it `leads`, the plan its decision is on standing as `plan` says, by the node
/// pointed at while it does not.
fn scene<'r>(
    run: &'r RunInView,
    frame: &RunFrame,
    pointed: &NodeFrame,
    leads: bool,
    plan: Plan,
) -> Scene<'r> {
    match leads {
        true => led(run, frame, pointed, plan),
        false => pointed_at(run, frame, pointed),
    }
}

/// The scene the run stands as while it leads, by its phase.
fn led<'r>(run: &'r RunInView, frame: &RunFrame, pointed: &NodeFrame, plan: Plan) -> Scene<'r> {
    match &frame.phase {
        RunPhase::Created | RunPhase::Running => moving(run, frame, pointed),
        RunPhase::Waiting { on } => waiting(run, frame, pointed, on, plan),
        RunPhase::Finished => Scene::Receipt,
        RunPhase::Failed { .. }
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. }
        | RunPhase::Broken { .. } => Scene::Ended,
    }
}

/// The scene of the node `pointed` while the run does not lead: on a run that ended
/// otherwise than finished, its end where the node is the one the run stands on at its
/// end, as its pointer goes there while it leads, and the node's own scene at any other;
/// on any other run, by the node's standing.
fn pointed_at<'r>(run: &'r RunInView, frame: &RunFrame, pointed: &NodeFrame) -> Scene<'r> {
    match &frame.phase {
        RunPhase::Failed { .. }
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. }
        | RunPhase::Broken { .. } => {
            let at_its_end = lead::node_led(frame, run.run.state(), None);
            match at_its_end.as_ref() == Some(&pointed.id) {
                true => Scene::Ended,
                false => Scene::Node,
            }
        }
        RunPhase::Created | RunPhase::Running | RunPhase::Waiting { .. } | RunPhase::Finished => {
            own(run, frame, pointed)
        }
    }
}

/// The scene of a run whose log says it moves, by the word its frame reads: no process
/// drives it; its process asks at its own terminal; nothing ran yet; else the node pointed
/// at stands at work, an answer awaiting the resume that acts on it among it. A word its
/// frame gives no run that moves reads as moving.
fn moving<'r>(run: &'r RunInView, frame: &RunFrame, pointed: &NodeFrame) -> Scene<'r> {
    match frame.standing {
        Standing::Stalled => Scene::Standstill(Standstill::NoProcess),
        Standing::NeedsYou => Scene::AskedElsewhere,
        Standing::Created => Scene::Work,
        Standing::Running
        | Standing::Waiting
        | Standing::Finished
        | Standing::Reported
        | Standing::Failed
        | Standing::Rerouted
        | Standing::Stopped
        | Standing::Promoted
        | Standing::Broken
        | Standing::Skipped
        | Standing::NeverRan => at_work(run, pointed),
    }
}

/// The scene of a run parked on `on`: what the pause standing asks, the plan its decision is
/// on standing as `plan` says, and the node pointed at at work, still, for a node that waits
/// with no pause standing.
fn waiting<'r>(
    run: &'r RunInView,
    frame: &RunFrame,
    pointed: &NodeFrame,
    on: &WaitingOn,
    plan: Plan,
) -> Scene<'r> {
    match on {
        WaitingOn::Node {
            reason: Some(pause),
            ..
        }
        | WaitingOn::Run { reason: pause } => paused(run, frame, pointed, pause, plan),
        WaitingOn::Node { reason: None, .. } => at_work(run, pointed),
    }
}

/// The scene of a run parked on `pause`: what it asks a person, the plan its decision is on
/// standing as `plan` says, its pull request, the node pointed at at work, still, while it
/// waits on a clock or on what is not read yet, or still where nothing here answers it.
fn paused<'r>(
    run: &'r RunInView,
    frame: &RunFrame,
    pointed: &NodeFrame,
    pause: &PauseReason,
    plan: Plan,
) -> Scene<'r> {
    match pause {
        PauseReason::Escalation(_)
        | PauseReason::NodeFailed { .. }
        | PauseReason::BudgetExhausted { .. }
        | PauseReason::LoopOverrun { .. } => decided(run, pointed, plan),
        PauseReason::Questions { .. } | PauseReason::AnswersRefused { .. } => {
            match run.awaited_round(frame) {
                Some(_) => Scene::Round,
                None => at_work(run, pointed),
            }
        }
        PauseReason::ExternalGate { .. } | PauseReason::PullRequestClosed { .. } => {
            Scene::PullRequest
        }
        PauseReason::AwaitingReset { .. } => at_work(run, pointed),
        // While the parent leads, its stage shows the child in its place; the parent, where it
        // is the run shown, has nothing of its own to answer and stands on its child's pause.
        PauseReason::ChildPaused { .. } => Scene::Standstill(Standstill::Unanswered),
        PauseReason::NoRoom { .. } => Scene::Standstill(Standstill::NoRoom),
        PauseReason::UncertainOrphans { .. } | PauseReason::Blocked { .. } => {
            Scene::Standstill(Standstill::HalfDone)
        }
        PauseReason::Cancelled
        | PauseReason::CancelledAfterCrash
        | PauseReason::GateAborted { .. }
        | PauseReason::Recorded { .. } => Scene::Standstill(Standstill::Stopped),
    }
}

/// The scene of a run parked on a decision: the decision once read for the pause it stands
/// at, the plan it is on standing as `plan` says; the node pointed at at work, still, while
/// the decision the engine rebuilds is not read yet or could not be; still where it rebuilds
/// none.
fn decided<'r>(run: &'r RunInView, pointed: &NodeFrame, plan: Plan) -> Scene<'r> {
    match run.awaited_decision() {
        Some(decision) => asked(run, decision, plan),
        None => match run.run.menu() {
            true => at_work(run, pointed),
            false => Scene::Standstill(Standstill::Unanswered),
        },
    }
}

/// The scene `decision` of `run` stands as, by the menu of the escalation the engine
/// rebuilt: a gate's desk where it shows a plan and the plan stands in front, as `plan`
/// says, or the change it counted holds no file, so the plan leads; else its card on what
/// it shows and its change; a card resting on what any other menu names; a pull request a
/// forge reviews; still where it carries none.
fn asked<'r>(run: &'r RunInView, decision: &'r WaitingDecision, plan: Plan) -> Scene<'r> {
    let card = |grounds| Scene::Card(decision, grounds);
    match decision.escalation.menu() {
        Some(Menu::Gate(_) | Menu::Console(_)) => {
            let counted = decision
                .on
                .node()
                .and_then(|gate| run.change_counted_at(gate));
            match (desk::plan_among(&decision.shown), counted, plan) {
                (Some(_), _, Plan::InFront) | (Some(_), None | Some(0), Plan::Behind) => {
                    Scene::Desk
                }
                (Some(_), Some(_), Plan::Behind) | (None, None | Some(_), _) => {
                    card(Grounds::Shown)
                }
            }
        }
        Some(Menu::Failure(menu)) => card(rests_on(&menu.failure)),
        Some(Menu::Departure(_)) => card(Grounds::Departure),
        Some(Menu::Scope(_)) => card(Grounds::Scope),
        Some(Menu::Budget(_) | Menu::LoopCap(_)) => card(Grounds::Cap),
        Some(Menu::Published(_)) => Scene::PullRequest,
        None => Scene::Standstill(Standstill::Unanswered),
    }
}

/// The scene of the node `pointed` of the run framed as `frame` while the run does not
/// lead, by its standing: as it stands at work while it works or waits on a clock; as it
/// ended once it did; its own scene otherwise.
fn own<'r>(run: &'r RunInView, frame: &RunFrame, pointed: &NodeFrame) -> Scene<'r> {
    match pointed.standing {
        Standing::Running | Standing::Waiting => at_work(run, pointed),
        Standing::Finished
        | Standing::Reported
        | Standing::Failed
        | Standing::Rerouted
        | Standing::Promoted
        | Standing::Broken => ended(run, frame, pointed),
        Standing::Created
        | Standing::NeedsYou
        | Standing::Stalled
        | Standing::Stopped
        | Standing::Skipped
        | Standing::NeverRan => Scene::Node,
    }
}

/// The scene of the node `pointed`, which ended, while the run does not lead: what it did,
/// the findings it posted or answered, as the findings scene shows them, where the run
/// framed as `frame` waits on no person at it; its own scene otherwise, which says what the
/// person's decision holds it on where the run waits on them there.
fn ended<'r>(run: &'r RunInView, frame: &RunFrame, pointed: &NodeFrame) -> Scene<'r> {
    let waited_at = frame.waits_on_a_person_at() == Some(&pointed.id);
    let found = reviews_or_fixes(&run.run.state().findings, &pointed.id);
    match (waited_at, found) {
        (false, true) => Scene::Findings,
        (false, false) | (true, true | false) => Scene::Node,
    }
}

/// The scene the node `node` of `run` stands as at work, by its kind as the run's
/// workflow declares it: a loop's board; else the findings where it posted or answered
/// one, its work otherwise.
fn at_work<'r>(run: &'r RunInView, node: &NodeFrame) -> Scene<'r> {
    let workflow = &run.run.manifest().workflow;
    let declared = workflow
        .iter_nodes()
        .find(|declared| declared.id == node.id);
    match declared.map(|declared| &declared.kind) {
        Some(NodeKind::Loop { .. }) => Scene::Board,
        Some(
            NodeKind::Prompt { .. }
            | NodeKind::Bash { .. }
            | NodeKind::Parallel { .. }
            | NodeKind::Check(_)
            | NodeKind::Executor { .. }
            | NodeKind::Gate { .. }
            | NodeKind::Workflow { .. }
            | NodeKind::PullRequest { .. },
        )
        | None => match reviews_or_fixes(&run.run.state().findings, &node.id) {
            true => Scene::Findings,
            false => Scene::Work,
        },
    }
}

/// Whether `node` posted one of the findings `findings` holds, or answered one.
fn reviews_or_fixes(findings: &FindingLedger, node: &NodeId) -> bool {
    findings.effective().iter().any(|posted| {
        let answers = findings.answers(posted.node.as_ref(), &posted.finding.id);
        posted.node.as_ref() == Some(node)
            || answers
                .iter()
                .any(|answer| answer.by.as_ref() == Some(node))
    })
}

#[cfg(test)]
mod tests;
