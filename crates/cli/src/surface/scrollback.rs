//! The terminal's own history, above the region.
//!
//! Two things a run sends up there: the work that stopped, which leaves
//! the pinned rows for good, and every diagnostic the run raises while
//! it is being drawn. Both go through here, and everything that goes
//! through here takes the region off the screen first — a line printed
//! around the region lands inside the rows it is redrawing and the next
//! redraw erases the copy it left behind.
//!
//! Which nodes have already gone up is kept here too, because it is the
//! same question: the region redraws many times over one settled node,
//! and the history takes it exactly once.

use std::collections::HashSet;
use std::io::Write;

use indicatif::MultiProgress;

use yunta_core::NodeId;

/// What is above the region, and the one door onto it.
pub(super) struct Scrollback {
    /// The region's own rows, taken off the screen for as long as a
    /// write takes and put back after it. A handle on the rows the
    /// region draws, never a second set of them.
    region: MultiProgress,
    /// Where the history goes: a terminal's stderr for a person, a
    /// buffer for a test that reads what a person would have seen.
    out: Box<dyn Write + Send>,
    /// Nodes already sent up, so each one goes exactly once however many
    /// times the region redraws after it.
    gone: HashSet<NodeId>,
}

impl Scrollback {
    /// The history above `region`, written to `out`.
    pub(super) fn over(region: &MultiProgress, out: Box<dyn Write + Send>) -> Self {
        Self {
            region: region.clone(),
            out,
            gone: HashSet::new(),
        }
    }

    /// The nodes among `settled` that have not gone up yet, recorded as
    /// gone.
    ///
    /// Asking and recording are one step because they are one decision:
    /// split in two, a caller that asked and then did not write leaves a
    /// node that never reaches the history at all.
    pub(super) fn leaving(&mut self, settled: Vec<&NodeId>) -> HashSet<NodeId> {
        let leaving: HashSet<NodeId> = settled
            .into_iter()
            .filter(|id| !self.gone.contains(*id))
            .cloned()
            .collect();
        self.gone.extend(leaving.iter().cloned());
        leaving
    }

    /// Writes `lines` above the region, with its rows off the screen for
    /// as long as the write takes.
    pub(super) fn write(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        let Self { region, out, .. } = self;
        region.suspend(|| {
            for line in lines {
                super::write_line(out, line);
            }
        });
    }

    /// Forgets what has gone up — what a promotion successor needs,
    /// since it is a run of its own whose node ids are its own.
    pub(super) fn restart(&mut self) {
        self.gone.clear();
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::TerminalState;
    use yunta_core::RunId;
    use yunta_engine::{NodeStanding, NodeState, RunFrame, RunPhase};
    use yunta_testkit::{child_link, node_frame, run_frame, Captured};

    use crate::render::Glyphs;
    use crate::surface::region::Region;
    use crate::surface::{Screen, Watched};

    use super::*;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const CHILD: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");

    /// A region drawn into a buffer: no terminal to stand up, and every
    /// draw landing before the next assertion.
    fn region(term: &Watched, out: &Captured) -> Region {
        Region::open(
            Screen::immediate(term.clone()),
            Glyphs::Ascii,
            Box::new(out.clone()),
        )
        .expect("the region's row template parses")
    }

    fn node(id: &'static str, state: NodeStanding) -> yunta_engine::NodeFrame {
        node_frame(&NodeId::from_static(id), state)
    }

    fn running(id: &'static str) -> yunta_engine::NodeFrame {
        node(id, NodeStanding::Reached(NodeState::Running { attempt: 1 }))
    }

    fn finished(id: &'static str) -> yunta_engine::NodeFrame {
        node(
            id,
            NodeStanding::Reached(NodeState::Finished {
                outcome: "exit 0".to_string(),
                tokens: yunta_core::events::TokenUsage::default(),
            }),
        )
    }

    fn frame(nodes: Vec<yunta_engine::NodeFrame>) -> RunFrame {
        RunFrame {
            phase: RunPhase::Running,
            nodes,
            ..run_frame(&RUN)
        }
    }

    #[test]
    fn a_node_that_stops_working_leaves_the_region_and_lands_above_it() {
        let term = Watched::sized(16, 80);
        let above = Captured::default();
        let mut region = region(&term, &above);

        region.show(&frame(vec![running("plan")]), &RUN, false);
        assert!(term.shown().contains("plan"), "{}", term.shown());
        assert_eq!(above.text(), "", "nothing has stopped working yet");

        region.show(
            &frame(vec![finished("plan"), running("build")]),
            &RUN,
            false,
        );
        assert!(
            !term.shown().contains("plan"),
            "the finished node left the region: {}",
            term.shown()
        );
        assert!(
            term.shown().contains("build"),
            "the working node is still in it: {}",
            term.shown()
        );
        assert!(
            above.text().contains("plan — finished — exit 0"),
            "it landed above the region: {:?}",
            above.text()
        );
    }

    #[test]
    fn a_node_goes_up_once_however_often_the_region_redraws_after_it() {
        let term = Watched::sized(16, 80);
        let above = Captured::default();
        let mut region = region(&term, &above);

        for _ in 0..3 {
            region.show(&frame(vec![finished("plan")]), &RUN, false);
        }
        assert_eq!(
            above.text().matches("plan").count(),
            1,
            "{:?}",
            above.text()
        );
    }

    #[test]
    fn a_node_that_bore_children_takes_them_with_it_into_the_history() {
        let term = Watched::sized(16, 80);
        let above = Captured::default();
        let mut region = region(&term, &above);

        region.show(
            &RunFrame {
                children: vec![child_link(
                    &CHILD,
                    Some(&NodeId::from_static("compose")),
                    Some(TerminalState::Done),
                )],
                ..frame(vec![finished("compose")])
            },
            &RUN,
            false,
        );

        let left = above.text();
        let child = left
            .lines()
            .position(|line| line.contains(&format!("child run {CHILD}")))
            .expect("the child left the region with the node that bore it");
        assert_eq!(
            left.lines().position(|line| line.contains("compose —")),
            child.checked_sub(1),
            "and it is still under it: {left:?}"
        );
        assert!(
            left.contains("finished · child run"),
            "carrying how the child closed: {left:?}"
        );
    }

    #[test]
    fn a_promotion_successor_sends_up_the_nodes_its_predecessor_already_did() {
        let mut above = Scrollback::over(&MultiProgress::new(), Box::new(Captured::default()));
        let plan = NodeId::from_static("plan");
        assert_eq!(above.leaving(vec![&plan]).len(), 1);
        assert!(
            above.leaving(vec![&plan]).is_empty(),
            "a node the history already took"
        );

        // A successor's node ids are its own, and its own run has sent
        // none of them up.
        above.restart();
        assert_eq!(
            above.leaving(vec![&plan]).len(),
            1,
            "the successor's first node never reached the history"
        );
    }
}
