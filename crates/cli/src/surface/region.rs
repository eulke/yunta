//! The pinned region: a few rows of plain text held at the bottom of a
//! terminal while the work that finished scrolls away above them.
//!
//! It takes no alternate screen, no raw mode and no mouse capture, which
//! is why the reader keeps their scrollback, their text selection, and a
//! typed Ctrl-C that still reaches the one cancellation bridge. Nothing
//! it draws carries meaning a word beside it does not already carry, so
//! the same rows read with every glyph stripped.
//!
//! **Nothing writes past the region except through it.** A line printed
//! around it lands inside the rows it is redrawing and leaves a torn
//! copy behind, so everything that goes above it — the work that
//! graduates, and every diagnostic the run raises while it is drawn —
//! goes through the [`Scrollback`], which takes the region down, writes,
//! and puts it back.

use std::io::Write;

use indicatif::style::TemplateError;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle, TermLike};

use yunta_core::RunId;
use yunta_engine::RunFrame;

use crate::render::{truncate, Glyphs, LINE_WIDTH};

use super::scrollback::Scrollback;
use super::{view, Screen};

/// The region's rows carry text and nothing else: no bar, no percentage,
/// and above all no spinner. What says a node is alive is the age of its
/// last event, which is measured and grows while the node says nothing.
const ROW: &str = "{msg}";

/// A run drawn as rows that stay in place, and the work that has left
/// them.
pub(super) struct Region {
    multi: MultiProgress,
    /// The terminal the rows go on, kept so every redraw cuts them to
    /// the width that terminal has at that moment.
    screen: Screen,
    /// Always the first row, whatever else the region holds: a reader
    /// looking for whether they are needed looks in one place.
    demand: ProgressBar,
    /// The working nodes and their detail, rebuilt as nodes come and go.
    body: Vec<ProgressBar>,
    /// Always the last row.
    counters: ProgressBar,
    glyphs: Glyphs,
    /// The style every row is drawn with, parsed once so adding a row
    /// later cannot fail.
    style: ProgressStyle,
    /// The terminal's own history above the region, and the one door
    /// onto it.
    above: Scrollback,
}

impl Region {
    /// Opens a region on `screen`, with `scrollback` taking the lines
    /// that graduate out of it.
    ///
    /// The caller decides where both go: a real terminal and its stderr
    /// for a person, a buffer for a test that asserts on what a person
    /// would have seen.
    pub(super) fn open(
        screen: Screen,
        glyphs: Glyphs,
        scrollback: Box<dyn Write + Send>,
    ) -> Result<Self, TemplateError> {
        let style = ProgressStyle::with_template(ROW)?;
        let multi = MultiProgress::with_draw_target(screen.target());
        let demand = multi.add(row(&style));
        let counters = multi.add(row(&style));
        let above = Scrollback::over(&multi, scrollback);
        Ok(Self {
            multi,
            screen,
            demand,
            body: Vec::new(),
            counters,
            glyphs,
            style,
            above,
        })
    }

    /// Redraws the region from `frame`, sending up the nodes that stopped
    /// working since the last redraw.
    ///
    /// `answerable` says whether the run stopped on a menu of its own —
    /// which decides the command the demand line offers, and which only
    /// the run's frozen manifest can answer.
    pub(super) fn show(&mut self, frame: &RunFrame, run_id: &RunId, answerable: bool) {
        self.demand
            .set_message(self.fit(&view::demand_line(frame, run_id, answerable)));
        let rows: Vec<String> = view::working(frame)
            .into_iter()
            .flat_map(|node| view::node_rows(frame, node, self.glyphs))
            .map(|row| self.fit(&row))
            .collect();
        self.resize(rows.len());
        for (bar, text) in self.body.iter().zip(rows) {
            bar.set_message(text);
        }
        self.counters
            .set_message(self.fit(&view::counter_line(frame)));
        // Last, so the region a graduating line is written above already
        // shows the work it left: a node is never both in the region and
        // in the scrollback at once.
        self.graduate(frame);
    }

    /// Writes one diagnostic into the terminal's history above the
    /// region.
    ///
    /// Whole, where the region's own rows are cut: a row of the region
    /// is cut because a wrapped one costs it the row count it redraws
    /// by, and this is not one of its rows. It is a sentence in the
    /// scrollback, which a terminal may lay out over as many rows as it
    /// takes without anything redrawing over it.
    pub(super) fn note(&mut self, line: &str) {
        self.above.write(&[line.to_string()]);
    }

    /// Forgets which nodes have graduated — what a promotion successor
    /// needs, since it is a run of its own whose node ids are its own.
    pub(super) fn restart(&mut self) {
        self.above.restart();
    }

    /// Takes the region off the terminal, leaving everything above it
    /// where it is. What redraws next puts the region back.
    ///
    /// This is how the region gives the screen to a prompt: the rows
    /// are gone before the prompt draws its first one, and what the
    /// person answers on is theirs alone.
    pub(super) fn lower(&self) {
        // A terminal that refuses the sequence clearing the region is a
        // terminal nothing could be reported on either.
        drop(self.multi.clear());
    }

    /// Takes the region down for good, leaving the terminal as it was
    /// found.
    ///
    /// The rows are cleared and then cut off from the terminal, in that
    /// order. A row still connected to a terminal when it is dropped
    /// draws itself one last time, which would put the region back on
    /// the terminal it had just been taken off — right under the block
    /// that closes the run out.
    pub(super) fn close(self) {
        self.lower();
        self.multi.set_draw_target(ProgressDrawTarget::hidden());
    }

    /// Sends every node that stopped working since the last redraw up
    /// into the history above, in the workflow's own declaration order.
    fn graduate(&mut self, frame: &RunFrame) {
        let leaving = self.above.leaving(view::settled_nodes(frame));
        let lines: Vec<String> = frame
            .nodes
            .iter()
            .filter(|node| leaving.contains(&node.id))
            .flat_map(|node| view::graduation(frame, node, self.glyphs))
            .map(|line| self.fit(&line))
            .collect();
        self.above.write(&lines);
    }

    /// Grows or shrinks the body to `rows` rows, keeping the demand line
    /// first and the counters last.
    fn resize(&mut self, rows: usize) {
        while self.body.len() > rows {
            if let Some(spare) = self.body.pop() {
                self.multi.remove(&spare);
            }
        }
        while self.body.len() < rows {
            self.body
                .push(self.multi.insert_before(&self.counters, row(&self.style)));
        }
    }

    /// `text` inside the width this region's rows have, so a row that
    /// would wrap is cut instead: a wrapped row costs the region its row
    /// count and leaves the line above it torn.
    fn fit(&self, text: &str) -> String {
        truncate(text, self.width(), self.glyphs)
            .trim_end()
            .to_string()
    }

    /// The cells one row of this region may take: the terminal's own
    /// width, and [`LINE_WIDTH`] wherever the terminal is wider.
    ///
    /// Both halves earn their place. Past the terminal's edge a row
    /// wraps, which is the tear [`Region::fit`] exists to prevent; below
    /// it, a layout that grew with the window would reflow the run under
    /// a reader every time they resized it, and the rows are written to
    /// be read at eighty cells. Asked on every redraw, so a window
    /// resized mid-run is followed.
    fn width(&self) -> usize {
        usize::from(self.screen.width()).min(LINE_WIDTH)
    }
}

/// One row of the region: no length, so nothing infers a percentage from
/// it, and the plain-text style.
fn row(style: &ProgressStyle) -> ProgressBar {
    ProgressBar::no_length().with_style(style.clone())
}

#[cfg(test)]
mod tests {
    use yunta_core::NodeId;
    use yunta_engine::{Counter, NodeStanding, NodeState, RunPhase, WaitingOn};
    use yunta_testkit::{child_link, node_frame, run_frame, Captured};

    use crate::surface::Watched;

    use super::*;

    const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
    const CHILD: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");

    /// A region drawn into a buffer: no terminal to stand up, and every
    /// draw landing before the next assertion.
    fn region(term: &Watched, scrollback: &Captured) -> Region {
        Region::open(
            Screen::immediate(term.clone()),
            Glyphs::Ascii,
            Box::new(scrollback.clone()),
        )
        .expect("the region's row template parses")
    }

    fn running(id: &'static str) -> yunta_engine::NodeFrame {
        node_frame(
            &NodeId::from_static(id),
            NodeStanding::Reached(NodeState::Running { attempt: 1 }),
        )
    }

    /// A run of `nodes` in `phase`, counted as the region counts them.
    fn frame(phase: RunPhase, nodes: Vec<yunta_engine::NodeFrame>) -> RunFrame {
        RunFrame {
            phase,
            flow: Counter {
                total: nodes.len(),
                ..Counter::default()
            },
            nodes,
            ..run_frame(&RUN)
        }
    }

    /// The run of `nodes`, with a child run still open under `bore`.
    fn composing(bore: &'static str, nodes: Vec<yunta_engine::NodeFrame>) -> RunFrame {
        RunFrame {
            children: vec![child_link(&CHILD, Some(&NodeId::from_static(bore)), None)],
            ..frame(RunPhase::Running, nodes)
        }
    }

    fn rows(term: &Watched) -> Vec<String> {
        term.shown().lines().map(str::to_string).collect()
    }

    #[test]
    fn the_demand_line_changes_what_it_says_and_never_where_it_says_it() {
        let term = Watched::sized(16, 80);
        let scrollback = Captured::default();
        let mut region = region(&term, &scrollback);

        region.show(
            &frame(RunPhase::Running, vec![running("plan")]),
            &RUN,
            false,
        );
        let quiet = rows(&term);
        assert_eq!(
            quiet.first().map(String::as_str),
            Some("nothing needs you"),
            "{quiet:?}"
        );

        region.show(
            &frame(
                RunPhase::Waiting {
                    on: WaitingOn::Node {
                        node: NodeId::from_static("plan"),
                        external_ref: None,
                        reason: None,
                    },
                },
                vec![running("plan")],
            ),
            &RUN,
            true,
        );
        let demanded = rows(&term);
        let first = demanded.first().map(String::as_str).unwrap_or_default();
        assert!(
            first.starts_with("needs you: "),
            "the demand line stays the first row and says what changed: {demanded:?}"
        );
        assert!(
            first.contains(&format!("yunta resolve-gate {RUN} <option>")),
            "it carries the command that answers it, whole: {first}"
        );
        assert!(
            first.contains("node `plan`"),
            "and what it is about: {first}"
        );
    }

    #[test]
    fn a_closed_region_leaves_nothing_of_itself_on_the_terminal() {
        let term = Watched::sized(16, 80);
        let scrollback = Captured::default();
        let mut region = region(&term, &scrollback);
        region.show(
            &frame(RunPhase::Running, vec![running("plan")]),
            &RUN,
            false,
        );
        assert!(!term.shown().is_empty(), "there is a region to take down");

        let drawn = term.written();
        region.close();
        assert_eq!(
            term.shown(),
            "",
            "the block that closes the run out lands on a terminal with nothing pinned to it"
        );
        assert_eq!(
            term.written(),
            drawn,
            "and nothing of the region reaches that terminal after it is taken down"
        );
    }

    #[test]
    fn a_row_is_cut_to_the_terminal_and_never_past_the_width_it_is_written_for() {
        // Below the terminal's edge a row wraps onto the row below,
        // which the next redraw does not account for: the region loses
        // its row count and tears. Above the width the rows are written
        // for, the layout would reflow under a reader every time they
        // resized the window.
        const PARKED: &str = "a-node-with-a-name-long-enough-to-fill-a-row";
        for columns in [40u16, 200] {
            let term = Watched::sized(16, columns);
            let scrollback = Captured::default();
            let mut region = region(&term, &scrollback);
            region.show(
                &frame(
                    RunPhase::Waiting {
                        on: WaitingOn::Node {
                            node: NodeId::from_static(PARKED),
                            external_ref: None,
                            reason: None,
                        },
                    },
                    vec![running(PARKED)],
                ),
                &RUN,
                true,
            );
            let room = usize::from(columns).min(LINE_WIDTH);
            let drawn = rows(&term);
            assert_eq!(
                drawn.len(),
                2 + region.body.len(),
                "on a terminal {columns} wide the region took more rows than it drew, which is \
                 a row that wrapped onto the one below: {drawn:?}"
            );
            for row in &drawn {
                assert!(
                    crate::render::cell_width(row) <= room,
                    "a row {} cells wide where {room} fit, on a terminal {columns} wide: {row:?}",
                    crate::render::cell_width(row)
                );
            }
            assert!(
                term.shown().contains("needs you"),
                "and the rows are still the run's: {}",
                term.shown()
            );
        }
    }

    #[test]
    fn a_child_run_is_drawn_under_the_node_that_bore_it_and_never_as_a_number() {
        let term = Watched::sized(16, 80);
        let scrollback = Captured::default();
        let mut region = region(&term, &scrollback);

        region.show(&composing("compose", vec![running("compose")]), &RUN, false);
        let drawn = rows(&term);
        let node = drawn
            .iter()
            .position(|row| row.contains("compose"))
            .expect("the node that bore the child is on the terminal");
        let child = drawn
            .get(node + 1)
            .map(String::as_str)
            .unwrap_or_default()
            .to_string();
        assert!(
            child.starts_with("    ") && child.contains(&format!("child run {CHILD}")),
            "the child sits under the node that bore it: {drawn:?}"
        );
        assert!(
            child.contains("still open"),
            "and says only what the parent's log says of it: {child}"
        );
        assert!(
            !child.contains('%'),
            "children are a tree, never a figure averaged over them: {child}"
        );
    }

    #[test]
    fn a_child_row_cut_to_a_narrow_terminal_still_says_where_the_child_stands() {
        const NARROW: u16 = 40;
        let term = Watched::sized(16, NARROW);
        let scrollback = Captured::default();
        let mut region = region(&term, &scrollback);

        region.show(&composing("compose", vec![running("compose")]), &RUN, false);
        let drawn = rows(&term);
        assert!(
            drawn.iter().any(|row| row.contains("still open")),
            "a row cut at the terminal's edge loses which child and never that there is \
             one: {drawn:?}"
        );
        for row in &drawn {
            assert!(
                crate::render::cell_width(row) <= usize::from(NARROW),
                "{row:?}"
            );
        }
    }

    #[test]
    fn the_counters_stay_the_last_row_as_nodes_come_and_go() {
        let term = Watched::sized(16, 80);
        let scrollback = Captured::default();
        let mut region = region(&term, &scrollback);

        for nodes in [
            vec![running("plan")],
            vec![running("plan"), running("build")],
            vec![running("build")],
        ] {
            region.show(&frame(RunPhase::Running, nodes), &RUN, false);
            let drawn = rows(&term);
            assert!(
                drawn.last().is_some_and(|row| row.starts_with("nodes ")),
                "{drawn:?}"
            );
        }
    }
}
