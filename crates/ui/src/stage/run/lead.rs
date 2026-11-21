//! See [`super`]. Who leads a run's screen, and how it is given back.
//!
//! A run's screen keeps two pointers, each kept by what it stands on, never by a position:
//! the node pointer, on a node of the run, and the finding pointer, on a finding it holds.
//! Each is the run's or the person's ([`PointedBy`]). The run leads its screen while both
//! are its own, which the stage derives each time it is asked and never keeps.
//!
//! While the run leads, each piece of it the state folds while its screen is the one in
//! sight, and each coming back to that screen, puts its pointers where the run stands; a
//! draw reads them as put, so news that came under another stage move them only once the
//! person comes back. The node pointer goes on the node the run waits on a person at,
//! when one waits, as the run's frame says it (`RunFrame::waits_on_a_person_at`), which
//! the node's row is marked by too; else it stays on the node at work it stands on, while
//! that node works; else on the first node at work in the workflow's declared order; else,
//! at a pause or an end that names no node, it stays where it was; else on the last node
//! that ran. The finding pointer goes on the first finding no node answered, else on the
//! last.
//!
//! A person takes the node pointer by moving it along the nodes, by going up from a child
//! to the node of its parent that bore it, or by putting the prompt in place aside, on the
//! node it waits at; and
//! the finding pointer by moving it along the findings; from then the run's news move
//! neither pointer. `j k` move along the findings where the pane lists them,
//! along the nodes anywhere else; `tab` hands them to the nodes there, and back. Either
//! goes around at its ends. `f` gives both back to the run, which puts them where it stands
//! at once. What the person took stays with the run's stage under every stage opened over
//! it; leaving the run for the field lets the stage go, and what was taken with it.

use surco::words::Standing;
use yunta_core::events::findings::FindingLedger;
use yunta_core::{FindingId, NodeId};
use yunta_render::screen::Along;
use yunta_replay::frame::{RunFrame, RunPhase, WaitingOn};
use yunta_replay::RunState;

use super::scene::ComposedPane;
use super::{Framed, RunStage};
use crate::stage::Want;

/// Who moves a pointer of a run's screen, and what it stands on: the run, where it last
/// put it, none before it put it anywhere; or the person, where they moved it. Not a
/// position among rows: a pointer is kept by the node or the finding it stands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PointedBy<T> {
    TheRun(Option<T>),
    ThePerson(T),
}

impl<T> PointedBy<T> {
    /// Whether the run moves it.
    pub(super) fn is_the_runs(&self) -> bool {
        matches!(self, PointedBy::TheRun(_))
    }

    /// What it stands on, where it stands on anything.
    pub(super) fn on(&self) -> Option<&T> {
        match self {
            PointedBy::TheRun(on) => on.as_ref(),
            PointedBy::ThePerson(on) => Some(on),
        }
    }

    /// Given back to the run, where the person held it, for the run to put where it stands;
    /// the run's own stays where the run put it.
    pub(super) fn given_back(&mut self) {
        if let PointedBy::ThePerson(_) = self {
            *self = PointedBy::TheRun(None);
        }
    }
}

impl RunStage {
    /// What `j` (`by` 1) or `k` (`by` -1) does on the run `framed`: the finding pointer moved
    /// along the findings where the pane lists them and `j k` are theirs, the node pointer
    /// along the nodes anywhere else.
    pub(super) fn along(&mut self, by: isize, framed: Option<&Framed<'_>>) -> Vec<Want> {
        match self.j_and_k_move(framed) {
            Along::Findings => self.moved_along_the_findings(by, framed),
            Along::Nodes => self.moved(by, framed),
        }
    }

    /// What `j k` move on the run `framed`: the findings where its pane lists them, until
    /// `tab` handed them to the nodes; the nodes anywhere else.
    pub(super) fn j_and_k_move(&self, framed: Option<&Framed<'_>>) -> Along {
        match self.lists_the_findings(framed) {
            true => self.along,
            false => Along::Nodes,
        }
    }

    /// What `tab` does on the run `framed`: where the pane lists the run's findings, `j k`
    /// handed from the findings to the nodes, or back; nothing anywhere else.
    pub(super) fn tabbed(&mut self, framed: Option<&Framed<'_>>) -> Vec<Want> {
        if self.lists_the_findings(framed) {
            self.along = match self.along {
                Along::Findings => Along::Nodes,
                Along::Nodes => Along::Findings,
            };
        }
        Vec::new()
    }

    /// Whether the pane of the run `framed` lists the run's findings: its scene composes the
    /// findings' pane.
    fn lists_the_findings(&self, framed: Option<&Framed<'_>>) -> bool {
        let scene = framed.and_then(|framed| self.scene_of(framed));
        scene.map(|scene| scene.composes()) == Some(ComposedPane::Findings)
    }

    /// The finding pointer taken by the person and moved `by` findings along the run
    /// `framed`'s, in the order they were posted, around at either end.
    fn moved_along_the_findings(&mut self, by: isize, framed: Option<&Framed<'_>>) -> Vec<Want> {
        let Some(framed) = framed else {
            return Vec::new();
        };
        let standing = framed.in_view.run.state.findings.effective();
        if standing.is_empty() {
            return Vec::new();
        }
        let pointed = self.finding_pointed_in(framed);
        let at = pointed
            .and_then(|(node, id)| {
                standing
                    .iter()
                    .position(|posted| posted.node == node && posted.finding.id == id)
            })
            .unwrap_or(0);
        let to = (at as isize + by).rem_euclid(standing.len() as isize) as usize;
        let posted = &standing[to];
        self.finding = PointedBy::ThePerson((posted.node.clone(), posted.finding.id.clone()));
        Vec::new()
    }

    /// The finding a key acts on among the run `framed`'s findings: the one pointed at where
    /// the run holds it, else where the run would put it.
    pub(super) fn finding_pointed_in(
        &self,
        framed: &Framed<'_>,
    ) -> Option<(Option<NodeId>, FindingId)> {
        let findings = &framed.in_view.run.state.findings;
        let standing = findings.effective();
        let held = |(node, id): &(Option<NodeId>, FindingId)| {
            standing
                .iter()
                .any(|posted| posted.node == *node && posted.finding.id == *id)
        };
        self.finding
            .on()
            .cloned()
            .filter(held)
            .or_else(|| finding_led(findings))
    }

    /// The pointer taken by the person and moved `by` nodes along the nodes of the run
    /// `framed`, around at either end, the pane of the node it lands on shown from its first
    /// line where it is another node's.
    fn moved(&mut self, by: isize, framed: Option<&Framed<'_>>) -> Vec<Want> {
        let Some(framed) = framed else {
            return Vec::new();
        };
        let nodes: Vec<&NodeId> = framed.frame.nodes.iter().map(|node| &node.id).collect();
        if nodes.is_empty() {
            return Vec::new();
        }
        let pointed = self.pointed_in(framed);
        let at = pointed
            .as_ref()
            .and_then(|pointed| nodes.iter().position(|node| *node == pointed))
            .unwrap_or(0);
        let to = (at as isize + by).rem_euclid(nodes.len() as isize) as usize;
        let node = nodes[to].clone();
        if pointed.as_ref() != Some(&node) {
            self.scroll.rewound();
        }
        self.node = PointedBy::ThePerson(node);
        self.feed.back_at_its_end();
        Vec::new()
    }

    /// The lead given back to the run `framed`: each pointer the person held is the run's
    /// again, put where the run stands, the feed at its end again, and the pane of the node
    /// the person pointed at goes for the run's, from its first line; where the run stands
    /// parked on a child, the child shown in its place, and where a promotion handed its
    /// work to a successor, the successor.
    pub(super) fn given_back(&mut self, framed: Option<&Framed<'_>>) -> Vec<Want> {
        if !self.node.is_the_runs() {
            self.scroll.rewound();
        }
        self.node.given_back();
        self.finding.given_back();
        self.feed.back_at_its_end();
        if let Some(framed) = framed {
            self.led(framed);
            self.onto_its_child(framed);
            self.onto_its_successor(framed);
        }
        Vec::new()
    }
}

/// Where the run puts its node pointer in `frame`, `state` the run it frames and `kept`
/// the node it last put it on: the node it waits on a person at, as the frame says it;
/// else `kept` while it works; else the first node at work in declared order; else, at a
/// pause or an end that names no node, `kept`; else the node that ran last. None for a run
/// whose nodes never ran and where none waits.
pub(super) fn node_led(
    frame: &RunFrame,
    state: &RunState,
    kept: Option<&NodeId>,
) -> Option<NodeId> {
    let first = |standing: Standing| {
        frame
            .nodes
            .iter()
            .find(|node| node.standing == standing)
            .map(|node| &node.id)
    };
    let at_work = |kept: &&NodeId| {
        frame
            .nodes
            .iter()
            .any(|node| node.id == **kept && node.standing == Standing::Running)
    };
    let held = |kept: &&NodeId| frame.nodes.iter().any(|node| node.id == **kept);
    frame
        .waits_on_a_person_at()
        .or_else(|| kept.filter(at_work))
        .or_else(|| first(Standing::Running))
        .or_else(|| kept.filter(held).filter(|_| names_no_node(frame)))
        .or_else(|| last_ran(frame, state))
        .cloned()
}

/// Where the run puts its finding pointer among `findings`, each by the node that posted
/// it (none for one the engine posted about the run) and its id among that node's: the
/// first, in the order they were posted, that no node answered; else the last. None while
/// it holds none.
pub(super) fn finding_led(findings: &FindingLedger) -> Option<(Option<NodeId>, FindingId)> {
    let standing = findings.effective();
    let unanswered = standing.iter().find(|posted| {
        findings
            .answers(posted.node.as_ref(), &posted.finding.id)
            .is_empty()
    });
    unanswered
        .or(standing.last())
        .map(|posted| (posted.node.clone(), posted.finding.id.clone()))
}

/// Whether the run `frame` stands where no node is named: paused on a pause that names
/// none (a cap, no room, a stop), stalled with no process driving it, or ended.
fn names_no_node(frame: &RunFrame) -> bool {
    let stands = match &frame.phase {
        RunPhase::Waiting {
            on: on @ WaitingOn::Run { .. },
        } => on.names().is_none(),
        RunPhase::Finished
        | RunPhase::Failed { .. }
        | RunPhase::Cancelled
        | RunPhase::Promoted { .. }
        | RunPhase::Broken { .. } => true,
        RunPhase::Waiting {
            on: WaitingOn::Node { .. },
        }
        | RunPhase::Created
        | RunPhase::Running => false,
    };
    stands || frame.standing == Standing::Stalled
}

/// The node of `frame` that ran last: the one whose latest start or end comes last in
/// `state`'s log.
fn last_ran<'f>(frame: &'f RunFrame, state: &RunState) -> Option<&'f NodeId> {
    frame
        .nodes
        .iter()
        .filter_map(|node| {
            let record = state.nodes.get(&node.id)?;
            let last = record.last_started.max(record.last_terminal)?;
            Some((last, &node.id))
        })
        .max_by_key(|(last, _)| *last)
        .map(|(_, node)| node)
}

#[cfg(test)]
mod tests;
