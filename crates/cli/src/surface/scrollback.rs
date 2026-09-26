//! The terminal's own history, above the region.
//!
//! Two things a run sends up there: the moments that closed something,
//! which the chronicle picks, and every diagnostic the run raises while
//! it is being drawn. Both go through here, and everything that goes
//! through here takes the region off the screen first — a line printed
//! around the region lands inside the rows it is redrawing and the next
//! redraw erases the copy it left behind.
//!
//! It remembers nothing. What has already gone up is how far the
//! chronicle has been read, which the painter counts; a history that
//! deduced it by comparing frames needed a set of node ids, and that is
//! where a real defect lived — a node that settles twice is two moments
//! and owes two lines.

use std::io::Write;

use indicatif::MultiProgress;

/// What is above the region, and the one door onto it.
pub(super) struct Scrollback {
    /// The region's own rows, taken off the screen for as long as a
    /// write takes and put back after it. A handle on the rows the
    /// region draws, never a second set of them.
    region: MultiProgress,
    /// Where the history goes: a terminal's stderr for a person, a
    /// buffer for a test that reads what a person would have seen.
    out: Box<dyn Write + Send>,
}

impl Scrollback {
    /// The history above `region`, written to `out`.
    pub(super) fn over(region: &MultiProgress, out: Box<dyn Write + Send>) -> Self {
        Self {
            region: region.clone(),
            out,
        }
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
}

#[cfg(test)]
mod tests {
    use yunta_core::{NodeId, RunId};
    use yunta_engine::{NodeStanding, NodeState, RunFrame, RunPhase};
    use yunta_testkit::{node_frame, run_frame};
    use yunta_testkit_core::Captured;

    use crate::render::Glyphs;
    use crate::surface::region::Region;
    use crate::surface::{Screen, Watched};

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

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

        // What closed is put here by the chronicle, not deduced by
        // comparing frames: the region shows what is open, and this is
        // what left it.
        region.record(&["+ plan — finished — exit 0".to_string()]);
        region.show(&frame(vec![running("build")]), &RUN, false);
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
}
