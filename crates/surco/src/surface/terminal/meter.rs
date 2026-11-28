//! The meter on a terminal: one row, its parts joined by the separator, which lets them go
//! whole where it passes the edge rather than be cut.
//!
//! The bars go first, since the figures beside them say in numbers what they show; then each
//! time from the last. A part goes whole, so a cut never falls inside a figure: only a count
//! wider than the row alone is cut, at the edge. A screen short of rows draws the figures
//! alone whatever its width: its look, not its width, lets the bars go.

use super::Terminal;
use crate::bars::sparkline;
use crate::components::Meter;
use crate::glyphs::Glyphs;
use crate::line::{Line, Tone};
use crate::width::cell_width;

impl Meter {
    /// The whole row, every part said: what Markdown and a page say of a meter, and what a
    /// terminal draws where it fits.
    pub(in crate::surface) fn said(&self, glyphs: Glyphs) -> Line {
        self.row(glyphs, true, self.times.len())
    }

    /// The count, then the bars when `bars` says so, then the first `times` of its times,
    /// each after the separator; a person's time in the person's tone, each label muted.
    fn row(&self, glyphs: Glyphs, bars: bool, times: usize) -> Line {
        let mut row = Line::new().plain(self.count.as_str());
        if bars && !self.bars.is_empty() {
            row = row.plain(" ").push(Tone::Running, self.bars(glyphs));
        }
        let sep = glyphs.spaced_sep();
        for time in self.times.iter().take(times) {
            let tone = match time.by_person {
                true => Tone::NeedsYou,
                false => Tone::Plain,
            };
            row = row
                .push(Tone::Faint, sep.as_str())
                .push(tone, time.figure.as_str())
                .push(Tone::Muted, format!(" {}", time.label));
        }
        row
    }

    /// One bar a node, each against the largest. A node that spent nothing is the lowest
    /// step, also where no node spent anything: the count beside the bars says the run spent
    /// nothing, where a sparkline alone draws a window with nothing to scale in its empty
    /// step.
    fn bars(&self, glyphs: Glyphs) -> String {
        match self.bars.iter().any(|bar| *bar > 0.0) {
            true => sparkline(&self.bars, self.bars.len(), glyphs),
            false => glyphs.ramp()[0].to_string().repeat(self.bars.len()),
        }
    }
}

impl Terminal {
    /// The meter's row at this look's width: whole where it fits; else as
    /// [`Terminal::meter_figures`] draws it.
    pub fn meter(&self, meter: &Meter) -> Line {
        let whole = meter.said(self.look.glyphs);
        match cell_width(&whole.text()) <= self.look.width.cells() {
            true => whole,
            false => self.meter_figures(meter),
        }
    }

    /// The meter's row in figures alone, its bars given way: as many of its times, from the
    /// first, as fit beside the count at this look's width, a count wider than the row cut.
    /// What a screen short of rows draws, whose look lets the bars go wherever they would fit.
    pub fn meter_figures(&self, meter: &Meter) -> Line {
        let glyphs = self.look.glyphs;
        let cells = self.look.width.cells();
        let fits = |row: &Line| cell_width(&row.text()) <= cells;
        (0..=meter.times.len())
            .rev()
            .map(|times| meter.row(glyphs, false, times))
            .find(fits)
            .unwrap_or_else(|| meter.row(glyphs, false, 0).cut(cells, glyphs))
    }
}
