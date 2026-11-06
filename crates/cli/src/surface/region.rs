//! The pinned region: a few rows of text held at the bottom of a
//! terminal while the work that finished scrolls away above them — the
//! row that asks for the person while something does, a row for each
//! node at work, and a footer that always says whether the run needs
//! them.
//!
//! It is the one place that keeps what a session said: the last frame
//! for each working node, which it lets go when the node stops working.
//! A frame from another run is never drawn, so a child's words stay with
//! the child.
//!
//! It takes no alternate screen, no raw mode and no mouse capture, which
//! is why the reader keeps their scrollback, their text selection, and a
//! typed Ctrl-C that still reaches the one cancellation bridge. Nothing
//! it draws carries meaning a word beside it does not already carry, so
//! the same rows read with every glyph and every color stripped.
//!
//! **Nothing writes past the region except through it.** A line printed
//! around it lands inside the rows it is redrawing and leaves a torn
//! copy behind, so everything that goes above it — the work that
//! graduates, and every diagnostic the run raises while it is drawn —
//! goes through the [`Scrollback`], which takes the region down, writes,
//! and puts it back.

use std::collections::HashMap;
use std::io::Write;

use indicatif::style::TemplateError;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle, TermLike};
use surco::glyphs::Glyphs;
use surco::line::Line;
use surco::motion::Motion;
use surco::paint::Paint;
use yunta_core::{NodeId, RunId};
use yunta_engine::said::Said;
use yunta_engine::RunFrame;

use super::scrollback::Scrollback;
use super::{view, Screen};

/// The region's rows carry text, and beside the footer's words the run's
/// furrow: no bar of one quantity and no percentage. The furrow is where each
/// node stands, which the counter beside it says in words. A running node's
/// spin turns a frame per event it says, and no clock turns it.
const ROW: &str = "{msg}";

/// A run drawn as rows that stay in place, and the work that has left
/// them.
pub(super) struct Region {
    multi: MultiProgress,
    /// The terminal the rows go on, kept so every redraw cuts them to
    /// the width that terminal has at that moment.
    screen: Screen,
    /// The first row while something needs the person, with the command
    /// that answers it; absent while nothing does.
    attention: Option<ProgressBar>,
    /// The working nodes and their detail, rebuilt as nodes come and go.
    body: Vec<ProgressBar>,
    /// Always the last row, and always answering whether the run needs
    /// the person: a reader looking for that looks in one place.
    footer: ProgressBar,
    glyphs: Glyphs,
    /// How the stream the rows go on is painted.
    paint: Paint,
    /// The style every row is drawn with, parsed once so adding a row
    /// later cannot fail.
    style: ProgressStyle,
    /// The terminal's own history above the region, and the one door
    /// onto it.
    above: Scrollback,
    /// Whether a running node's mark turns.
    motion: Motion,
    /// Whether the person asked for a bell when they are asked something.
    bell: bool,
    /// Whether the window's title says a person is being asked.
    told: bool,
    /// What a session of each working node said last, by run and node.
    /// Only a region keeps it, so nothing said reaches a surface that is
    /// not a terminal.
    said: HashMap<(RunId, NodeId), Said>,
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
        paint: Paint,
        scrollback: Box<dyn Write + Send>,
    ) -> Result<Self, TemplateError> {
        let style = ProgressStyle::with_template(ROW)?;
        let multi = MultiProgress::with_draw_target(screen.target());
        let footer = multi.add(row(&style));
        let above = Scrollback::over(&multi, scrollback);
        Ok(Self {
            multi,
            screen,
            attention: None,
            body: Vec::new(),
            footer,
            glyphs,
            paint,
            style,
            above,
            motion: Motion::default(),
            bell: false,
            told: false,
            said: HashMap::new(),
        })
    }

    /// This region, turning as `motion` says.
    pub(super) fn moving(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    /// This region, ringing when a person is asked, if `bell` says to.
    pub(super) fn ringing(mut self, bell: bool) -> Self {
        self.bell = bell;
        self
    }

    /// Keeps what a session said, for the row of its node while the node
    /// works. The next redraw shows it.
    pub(super) fn hear(&mut self, said: Said) {
        self.said
            .insert((said.run_id.clone(), said.node_id.clone()), said);
    }

    /// Redraws the region from `frame`, sending up the nodes that stopped
    /// working since the last redraw.
    ///
    /// `answerable` says whether the run stopped on a menu of its own —
    /// which decides the command the attention row offers, and which
    /// only the run's frozen manifest can answer. `asking` is whom the
    /// engine asks at its own terminal, as a process that follows the run
    /// reads it off the registry; the row says so instead.
    pub(super) fn show(
        &mut self,
        frame: &RunFrame,
        run_id: &RunId,
        (answerable, asking): (bool, Option<&yunta_engine::Prompt>),
    ) {
        let attention = match asking {
            Some(prompt) => Some(view::asked(prompt, self.glyphs)),
            None => view::attention(frame, run_id, answerable, self.glyphs),
        }
        .map(|line| self.fit(&line));
        self.attend(attention);
        let working = view::working(frame);
        // What a node that stopped working said, and what another run said,
        // is let go: a node that works again says something new.
        self.said.retain(|(run, node), _| {
            run == run_id && working.iter().any(|working| &working.id == node)
        });
        let rows: Vec<String> = working
            .into_iter()
            .flat_map(|node| {
                let said = self.said.get(&(run_id.clone(), node.id.clone()));
                view::node_rows(frame, node, said, &self.look(), self.motion)
            })
            .map(|row| self.fit(&row))
            .collect();
        self.resize(rows.len());
        for (bar, text) in self.body.iter().zip(rows) {
            bar.set_message(text);
        }
        self.footer
            .set_message(self.fit(&view::footer(frame, &self.look())));
    }

    /// Tells the window a person is being asked, as a prompt takes the terminal:
    /// its title says so, and the bell rings if the person asked for one.
    pub(super) fn ask(&mut self, frame: &RunFrame, run_id: &RunId) {
        if self.told {
            return;
        }
        self.told = true;
        let title = view::title(frame, run_id, self.glyphs);
        let bell = if self.bell { surco::paint::BELL } else { "" };
        // A terminal that refuses the sequence is a terminal nothing could be
        // reported on either.
        drop(
            self.screen
                .signal(&format!("{}{bell}", surco::paint::title_saved(&title))),
        );
    }

    /// Gives the window back the title it had, once the person has answered.
    pub(super) fn answered(&mut self) {
        if std::mem::take(&mut self.told) {
            drop(self.screen.signal(surco::paint::TITLE_RESTORED));
        }
    }

    /// Puts the attention row at the top while there is something to
    /// say on it, and takes it off when there is not.
    fn attend(&mut self, said: Option<String>) {
        match (said, &self.attention) {
            (Some(said), Some(bar)) => bar.set_message(said),
            (Some(said), None) => {
                let bar = self.multi.insert(0, row(&self.style));
                bar.set_message(said);
                self.attention = Some(bar);
            }
            (None, _) => {
                if let Some(bar) = self.attention.take() {
                    self.multi.remove(&bar);
                }
            }
        }
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
    pub(super) fn close(mut self) {
        self.answered();
        self.lower();
        self.multi.set_draw_target(ProgressDrawTarget::hidden());
    }

    /// Writes one moment's rows into the terminal's history above the
    /// region.
    ///
    /// Which moments reach here is the chronicle's to say, not this
    /// surface's: it lays out what it is handed. The rows are cut to
    /// the terminal like the region's own, because a wrapped one would
    /// cost the region the row count it redraws by.
    pub(super) fn record(&mut self, rows: &[Line]) {
        let rows: Vec<String> = rows.iter().map(|row| self.fit(row)).collect();
        self.above.write(&rows);
    }

    /// The look of the rows this region draws, at the width they have
    /// now.
    pub(super) fn look(&self) -> surco::look::Look {
        surco::look::Look {
            glyphs: self.glyphs,
            paint: self.paint,
            width: crate::render::row_width(usize::from(self.screen.width())),
        }
    }

    /// Grows or shrinks the body to `rows` rows, keeping the attention
    /// row first and the footer last.
    fn resize(&mut self, rows: usize) {
        while self.body.len() > rows {
            if let Some(spare) = self.body.pop() {
                self.multi.remove(&spare);
            }
        }
        while self.body.len() < rows {
            self.body
                .push(self.multi.insert_before(&self.footer, row(&self.style)));
        }
    }

    /// `line` inside the width this region's rows have, painted: a row
    /// that would wrap is cut instead, because a wrapped row costs the
    /// region its row count and leaves the line above it torn. It is cut
    /// on the words and painted after, so the paint never counts as
    /// width.
    fn fit(&self, line: &Line) -> String {
        self.paint.line(&line.cut(self.width(), self.glyphs))
    }

    /// The cells one row of this region may take: the width a line on
    /// this terminal gets, never past the terminal's edge — past it a
    /// row wraps, which is the tear [`Region::fit`] exists to prevent.
    /// Asked on every redraw, so a window resized mid-run is followed.
    fn width(&self) -> usize {
        crate::render::row_width(usize::from(self.screen.width())).cells()
    }
}

/// One row of the region: no length, so nothing infers a percentage from
/// it, and the plain-text style.
fn row(style: &ProgressStyle) -> ProgressBar {
    ProgressBar::no_length().with_style(style.clone())
}

#[cfg(test)]
mod tests;
