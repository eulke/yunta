//! See [`super`]. The run in view's fold: the run read whole, then each piece of news of
//! it taken once, what its sessions said kept to the nodes at work, and when it stands
//! caught up with the log; and what it keeps, read for the pause it stands at: the decision
//! or the round it waits on, and the change a node counted.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use yunta_api::{
    waiting_round, Awaited, Document, FollowedRun, RunNews, WaitingDecision, WaitingQuestions,
};
use yunta_core::events::run::happening;
use yunta_core::events::{Shown, StoredEvent};
use yunta_core::port::Said;
use yunta_core::{NodeId, RunId, Seq};
use yunta_render::watched::Heard;
use yunta_replay::frame::{chronicle, Happening, Moment, RunFrame, RunPhase};

use super::{AwaitedReading, RunInView};

impl RunInView {
    /// `run` read whole with `events`: its moments their chronicle, and nothing heard,
    /// handed over or awaited yet.
    pub(super) fn read_whole(run: FollowedRun, events: &[StoredEvent]) -> RunInView {
        let mut read = RunInView {
            run,
            moments: Vec::new(),
            by_node: BTreeMap::new(),
            handed_over: BTreeMap::new(),
            heard: Heard::default(),
            missed: 0,
            awaited: None,
            remarks: Vec::new(),
            successor: None,
            no_successor: false,
        };
        read.told(chronicle(events));
        read
    }

    /// `moments` kept after those before them, each indexed under its node.
    fn told(&mut self, moments: Vec<Moment>) {
        for moment in moments {
            if let Some(node) = &moment.node {
                let at = self.moments.len();
                self.by_node.entry(node.clone()).or_default().push(at);
            }
            self.moments.push(moment);
        }
    }

    /// `news` of the run folded in: the moments the events that came are, its engine, what
    /// a session said with the words missed before it, a successor read whole and kept
    /// beside it, no successor come, what it waits on a person for, a document handed over
    /// and a remark.
    pub(super) fn took(&mut self, news: RunNews) {
        match news {
            RunNews::Events(_) | RunNews::Engine(_) => {
                let head = self.run.state.head();
                let moments = self.run.took(&news);
                self.told(moments);
                if self.run.state.head() != head {
                    self.heard_kept_to_work();
                }
            }
            RunNews::Said { said, missed } => {
                let repeated = self.heard.of(&said.run_id, &said.node_id) == Some(&said);
                if !repeated && !self.said_by_a_session_gone(&said) {
                    self.missed += missed;
                    self.heard.hear(said);
                }
            }
            RunNews::Successor { run, events } => {
                self.successor = Some(Box::new(RunInView::read_whole(*run, &events)));
            }
            RunNews::NoSuccessor => self.no_successor = true,
            RunNews::Awaited(awaited) => {
                let read = |(pause, awaited)| AwaitedReading::Read { pause, awaited };
                self.awaited = awaited.map(read);
            }
            RunNews::AwaitedUnread { pause, why } => {
                self.awaited = Some(AwaitedReading::Unread { pause, why });
            }
            RunNews::Document { node, document } => self.handed(node, document),
            RunNews::Remark(remark) => {
                if !self.remarks.contains(&remark) {
                    self.remarks.push(remark);
                }
            }
        }
    }

    /// What its sessions said kept to the nodes at work and those yet to start: a node that
    /// stopped working lets its words go, so working again it shows none of its last
    /// session's. Whether a node works reads no clock, so the frame is taken at the
    /// instant of the run's last event; a run with none has said nothing.
    fn heard_kept_to_work(&mut self) {
        let Some(at) = self.run.state.clock().last_at() else {
            return;
        };
        self.heard.kept_to(&self.run.frame(at), &self.run.run);
    }

    /// Whether `said` came before its node last started or last ended, by where it sits
    /// among the run's events: the words of a session that stopped working, which the
    /// node let go then. Handed again, as a following made anew hears a run's recent
    /// past, they are not heard again.
    fn said_by_a_session_gone(&self, said: &Said) -> bool {
        let Some(node) = self.run.state.nodes.get(&said.node_id) else {
            return false;
        };
        [node.last_started, node.last_terminal]
            .into_iter()
            .flatten()
            .any(|since| said.seq < since)
    }

    /// `document`, handed over by `node`, kept under it in place of the one it repeats:
    /// the same document by the same bytes, read or not.
    fn handed(&mut self, node: NodeId, document: Document) {
        let documents = self.handed_over.entry(node).or_default();
        let repeated = documents
            .iter_mut()
            .find(|kept| shown(kept) == shown(&document));
        match repeated {
            Some(kept) => *kept = document,
            None => documents.push(document),
        }
    }

    /// Whether it stands at its head in `heads`, every document its nodes handed over by
    /// then kept, read or not; where it stands parked on a person at `now` with a menu or a
    /// round, what it waits for is read for that pause; and where it closed promoted, its
    /// successor came, or none did.
    pub(super) fn caught_up(&self, heads: &BTreeMap<RunId, Seq>, now: DateTime<Utc>) -> bool {
        let at_head = self.run.state.head() == heads.get(&self.run.run).copied();
        let read_for = self.awaited.as_ref().map(|read| read.pause().seq());
        let promoted = matches!(self.run.frame(now).phase, RunPhase::Promoted { .. });
        at_head
            && self.handed_over_kept()
            && (!promoted || self.no_successor || self.successor.is_some())
            && self
                .parked_on_a_person(now)
                .is_none_or(|pause| read_for == Some(pause))
    }

    /// Whether every document its nodes handed over that a person reads whole is kept under
    /// the node that handed it over, read or not: what its following hands of it by the head
    /// it stands at, after the run read whole.
    fn handed_over_kept(&self) -> bool {
        Document::handed_over_in(&self.run.state)
            .iter()
            .all(|(node, due)| {
                let kept = self.handed_over.get(node).into_iter().flatten();
                kept.map(shown).any(|kept| kept == due)
            })
    }

    /// Where the log holds the event of the pause it stands parked on a person at, at
    /// `now`, as the follower names the pause it reads what a run waits on for: the
    /// `questions_asked` of the round a node waits on, or the `run_paused` of a decision
    /// with a menu. None while it stands parked on neither.
    fn parked_on_a_person(&self, now: DateTime<Utc>) -> Option<Seq> {
        let (_, paused_at) = self.run.state.run.paused()?;
        match waiting_round(&self.run.frame(now)) {
            Some(node) => self.round_asked_at(node),
            None => self.run.menu.then_some(paused_at),
        }
    }

    /// Where the log holds the `questions_asked` that opened the round `node` asked last,
    /// while no answer to it is recorded: once one is, the round waits on its resume, not on
    /// a person.
    fn round_asked_at(&self, node: &NodeId) -> Option<Seq> {
        let round = self.run.state.gates.get(node)?.rounds.last()?;
        round.answered.is_none().then_some(round.asked_at)
    }

    /// The decision it waits on a person for, as read for the `run_paused` it stands at:
    /// none while it stands at none, before what it waits on is read for that pause, and
    /// where it could not be read.
    pub(crate) fn awaited_decision(&self) -> Option<&WaitingDecision> {
        let paused_at = self.run.state.run.paused().map(|(_, at)| at);
        match &self.awaited {
            Some(AwaitedReading::Read {
                pause,
                awaited: Awaited::Decision(decision),
            }) => (Some(pause.seq()) == paused_at).then_some(&**decision),
            Some(AwaitedReading::Read {
                awaited: Awaited::Round(_),
                ..
            })
            | Some(AwaitedReading::Unread { .. })
            | None => None,
        }
    }

    /// The round of questions it waits on a person for, framed as `frame`, as read for the
    /// `questions_asked` that opened the round its node is parked on: none while it is
    /// parked on none, before that round is read, and where it could not be read.
    pub(crate) fn awaited_round(&self, frame: &RunFrame) -> Option<&WaitingQuestions> {
        let asked_at = waiting_round(frame).and_then(|node| self.round_asked_at(node));
        match &self.awaited {
            Some(AwaitedReading::Read {
                pause,
                awaited: Awaited::Round(round),
            }) => (Some(pause.seq()) == asked_at).then_some(&**round),
            Some(AwaitedReading::Read {
                awaited: Awaited::Decision(_),
                ..
            })
            | Some(AwaitedReading::Unread { .. })
            | None => None,
        }
    }

    /// Why what the run framed as `frame` waits on a person for could not be read, where it
    /// could not be read for the pause it stands parked on: the decision of its `run_paused`
    /// with a menu, or the round its node opened. None where it was read, where it is not
    /// read yet, and while it stands parked on neither.
    pub(crate) fn unreadable_ask(&self, frame: &RunFrame) -> Option<&str> {
        let Some(AwaitedReading::Unread { pause, why }) = &self.awaited else {
            return None;
        };
        let stands_at = match waiting_round(frame) {
            Some(node) => self.round_asked_at(node),
            None => self.run.state.run.paused().map(|(_, at)| at),
        };
        (Some(pause.seq()) == stands_at).then_some(why.as_str())
    }

    /// How many files the change the run last counted at `node` holds, as its chronicle
    /// says it (`the change counted: …`): none where it counted none there.
    pub(crate) fn change_counted_at(&self, node: &NodeId) -> Option<u32> {
        let moments = self.by_node.get(node)?;
        moments.iter().rev().find_map(|at| {
            let moment = self.moments.get(*at)?;
            match &moment.happening {
                Happening::Run(happening::Happening::ChangesCounted { files, .. }) => Some(*files),
                _ => None,
            }
        })
    }
}

/// What `document` names: a document by its bytes, read or not.
fn shown(document: &Document) -> &Shown {
    match document {
        Document::Read(read) => &read.shown,
        Document::Unread { shown, .. } => shown,
    }
}
