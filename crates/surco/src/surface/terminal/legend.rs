//! The legend on a terminal: the keys of what is on screen, in a row; on a screen, the
//! row a screen keeps for them, with the far keys at its far end where the row is wide enough;
//! under a prompt that says its own, as many rows as they take, folded between two keys.
//! A row too narrow for every key keeps the last, the key that lists every key; where a line
//! is typed no key lists every key, so the row keeps the first that fit and says more stand
//! after them.

use super::{at_ends, Terminal, APART};
use crate::components::{Binding, Legend};
use crate::line::{Line, Tone};
use crate::look::{Look, Width};
use crate::width::cell_width;

impl Legend {
    /// Each key strong and what it does muted, a faint separator between two,
    /// on as many lines as the width takes: a key is never parted from what
    /// it does.
    pub(in crate::surface) fn lines(&self, look: &Look) -> Vec<Line> {
        lines(&self.0.iter().collect::<Vec<_>>(), look)
    }

    /// Its lines in `cells` of `look`, as [`Legend::lines`] folds them: the keys under a
    /// prompt, a card's, a desk's or a list's, in the room the prompt leaves them.
    pub(in crate::surface::terminal) fn folded(&self, look: &Look, cells: usize) -> Vec<Line> {
        self.lines(&Look {
            width: Width::exact(cells),
            ..*look
        })
    }
}

impl Terminal {
    /// The row of keys a screen holds, at this look's width: the screen's keys, then the far
    /// keys, `anywhere`. On the design's wide screen, from a row of 98 cells, where a run's keys
    /// and the far keys stand whole apart, the far keys stand whole at its far end, and
    /// where the screen's keys do not all fit beside them, the first that fit stand with a mark
    /// that more stand after them. Narrower, the far keys are left out but the last, which
    /// lists every key and follows the screen's keys; where they do not all fit, the first keys
    /// that fit, a mark that more stand between, and that last key. A last key too wide even
    /// for that leaves the first keys that fit, cut, and one key alone too wide for the row is
    /// cut. With no far keys, as over a box or on a screen whose own keys end with the key that
    /// lists every key, the row is the screen's keys alone, their last kept where they do not
    /// all fit; a screen where a line is typed draws its own ([`Terminal::typed_legend`]).
    pub fn legend(&self, legend: &Legend, anywhere: &Legend) -> Line {
        let look = &self.look;
        let cells = look.width.cells();
        let mut keys: Vec<&Binding> = legend.0.iter().collect();
        if !self.wide() || anywhere.is_empty() {
            keys.extend(anywhere.0.last());
            return row(&keys, look);
        }
        let far: Vec<&Binding> = anywhere.0.iter().collect();
        let far = joined(&far, look).cut(cells, look.glyphs);
        let room = cells.saturating_sub(cell_width(&far.text()) + APART);
        at_ends(before_far(&keys, room, look), Some(far), cells)
    }

    /// The row of keys a screen where a line is typed holds, at this look's width: its keys
    /// alone, which say only what acts there; where they do not all fit, the first that fit
    /// and a mark that more stand after them, since none of them lists every key. A first key
    /// too wide even for that is cut.
    pub fn typed_legend(&self, legend: &Legend) -> Line {
        let keys: Vec<&Binding> = legend.0.iter().collect();
        before_far(&keys, self.look.width.cells(), &self.look)
    }
}

/// `keys` on as many lines as the width takes, each key beside what it does.
fn lines(keys: &[&Binding], look: &Look) -> Vec<Line> {
    let width = look.width.cells();
    let sep = look.glyphs.spaced_sep();
    let mut lines = Vec::new();
    let mut line = Line::new();
    for binding in keys {
        let said = spelled(binding, look);
        let used = cell_width(&line.text());
        if used > 0 && used + cell_width(&sep) + cell_width(&said.text()) > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.text().is_empty() {
            line = line.push(Tone::Faint, sep.as_str());
        }
        line = line.then(said);
    }
    if !line.text().is_empty() {
        lines.push(line);
    }
    lines
}

/// `keys` in one row of the look's width: every key where they all fit. Where they do not,
/// the first keys that fit, a mark that more stand between, and the last key, which leads to
/// the rest: what a screen offers last is the key that lists every key. A last key too wide
/// even for that leaves the first keys that fit, cut, and one key alone too wide for the row
/// is cut.
fn row(keys: &[&Binding], look: &Look) -> Line {
    let cells = look.width.cells();
    let mut whole = lines(keys, look).into_iter();
    let first_cut = whole.next().unwrap_or_default().cut(cells, look.glyphs);
    if whole.next().is_none() {
        return first_cut;
    }
    let sep = look.glyphs.spaced_sep();
    let Some((last, first)) = keys.split_last() else {
        return Line::new();
    };
    let tail = Line::new()
        .push(Tone::Faint, sep.as_str())
        .push(Tone::Faint, look.glyphs.ellipsis())
        .push(Tone::Faint, sep.as_str())
        .then(spelled(last, look));
    let room = cells.saturating_sub(cell_width(&tail.text()));
    let kept = first_fitting(first, room, look);
    match kept.text().is_empty() {
        true => first_cut,
        false => kept.then(tail),
    }
}

/// The screen's `keys` in `room` cells, before the far keys: every key where they all fit;
/// where they do not, the first that fit and a mark that more stand after them; a first key
/// too wide even for that is cut.
fn before_far(keys: &[&Binding], room: usize, look: &Look) -> Line {
    let every = joined(keys, look);
    if cell_width(&every.text()) <= room {
        return every;
    }
    let more = Line::new()
        .push(Tone::Faint, look.glyphs.spaced_sep())
        .push(Tone::Faint, look.glyphs.ellipsis());
    let kept = first_fitting(keys, room.saturating_sub(cell_width(&more.text())), look);
    match kept.text().is_empty() {
        true => every.cut(room, look.glyphs),
        false => kept.then(more),
    }
}

/// The first of `keys` that fit in `room` cells, a faint separator between two.
fn first_fitting(keys: &[&Binding], room: usize, look: &Look) -> Line {
    let sep = look.glyphs.spaced_sep();
    let mut row = Line::new();
    for binding in keys {
        let said = spelled(binding, look);
        let joined = match row.text().is_empty() {
            true => said,
            false => row.clone().push(Tone::Faint, sep.as_str()).then(said),
        };
        if cell_width(&joined.text()) > room {
            break;
        }
        row = joined;
    }
    row
}

/// `keys` on one line, however wide, a faint separator between two.
fn joined(keys: &[&Binding], look: &Look) -> Line {
    let sep = look.glyphs.spaced_sep();
    keys.iter()
        .enumerate()
        .fold(Line::new(), |line, (at, binding)| match at {
            0 => line.then(spelled(binding, look)),
            _ => line
                .push(Tone::Faint, sep.as_str())
                .then(spelled(binding, look)),
        })
}

/// One key, strong, and what it does, muted.
fn spelled(binding: &Binding, look: &Look) -> Line {
    Line::new()
        .push(Tone::Strong, binding.keys.spelled(look.glyphs))
        .push(Tone::Muted, format!(" {}", binding.does))
}

#[cfg(test)]
mod tests;
