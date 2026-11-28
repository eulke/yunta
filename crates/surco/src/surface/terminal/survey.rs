//! A survey at a terminal: what it says over its rows, folded at the width its column lends a
//! line said across it; then each list, its heading muted over its rows and a blank row between
//! two lists; then, after a blank row, the act's row. A row is its pointer's margin, its mark,
//! its label in a column two cells past the widest label of every list, and what stands for it:
//! words found, going on under themselves; a row its caller composed, cut at the edge; or a row
//! of words with one chosen, the chosen word where the values stand and its pointer two cells
//! before it. A line typed in place stands under its row where the values stand, in a box whose
//! label stands in its top edge, its hint muted under it.
//!
//! Held in fewer rows than all of it takes under what stands above it, as a name in block
//! letters, it gives way in order: the blank row under the sentence; what stands above it,
//! whole; then a window over what it says and its rows that keeps the act's row at its foot
//! and the row pointed at in sight. The window reaches down to the act's row where that keeps
//! the row pointed at, and opens at the row pointed at otherwise; the rows it hides are counted
//! over it and under it at the survey's first cell, the sentence and the headings never
//! counted, and a window never opens on a blank row. A row pointed at taller than what the
//! counts leave it gives way from its end, the hint under a line typed in place first and then
//! the box's rows, down to its own row; under that, the counts give way, the one under first.
//! Held so, a survey never takes more rows than it is given: the act's row is the last of
//! them, and what stands under the survey gives way from its end before it does.

use super::card::Placed;
use super::pane::Paned;
use super::Terminal;
use crate::components::{Laid, Survey, SurveyRow, SurveyValue, TypedInPlace};
use crate::line::{Hidden, Line, Tone};
use crate::width::{cell_width, wrap, INDENT};

/// The cells between the widest label and what stands for a row: two, as a checklist's
/// subjects stand apart from what they found.
const GAP: usize = 2;

/// The cells a row of words with one chosen takes before its first word: a cell where none is
/// lost before it, the pointer's and the cell before the word.
const CHOSEN_LEAD: usize = 3;

/// The cells a row's mark takes and the one after it.
const MARK: usize = 2;

/// A part of a survey as a window takes it: its lines, whether it is a row a person moves
/// along, which a window counts where it hides it, whether it is a blank row, which a window
/// never opens on, and where the cursor stands among its lines.
struct Unit {
    lines: Vec<Line>,
    counted: bool,
    blank: bool,
    pointed: bool,
    cursor: Option<(usize, usize)>,
}

impl Unit {
    fn blank() -> Unit {
        Unit {
            lines: vec![Line::new()],
            counted: false,
            blank: true,
            pointed: false,
            cursor: None,
        }
    }

    fn said(lines: Vec<Line>) -> Unit {
        Unit {
            lines,
            counted: false,
            blank: false,
            pointed: false,
            cursor: None,
        }
    }
}

impl Terminal {
    /// `survey` whole, as a document draws it.
    pub(super) fn survey(&self, survey: &Survey) -> Vec<Line> {
        let mut lines = flat(&self.survey_units(survey, true)).0;
        lines.push(self.survey_foot(survey));
        lines
    }

    /// `survey` held in `rows` rows under `above`, what stands over it (a blank row parting
    /// them already), and over `below`: all of it where it fits; else without the blank row
    /// under its sentence; else without what stands above; else in a window over its rows
    /// that keeps its foot and the row pointed at in sight. The cursor where a line is typed
    /// in it and stands in sight.
    pub(super) fn survey_held(
        &self,
        above: Vec<Line>,
        survey: &Survey,
        below: Vec<Line>,
        rows: usize,
    ) -> Paned {
        let foot = self.survey_foot(survey);
        let room = rows.saturating_sub(below.len() + 1);
        for (aired, kept_above) in [(true, true), (false, true), (false, false)] {
            let (lines, cursor) = flat(&self.survey_units(survey, aired));
            let over = match kept_above {
                true => above.len(),
                false => 0,
            };
            if over + lines.len() <= room {
                let mut drawn: Vec<Line> = match kept_above {
                    true => above.clone(),
                    false => Vec::new(),
                };
                drawn.extend(lines);
                return held(
                    drawn,
                    foot,
                    below,
                    cursor.map(|(row, cell)| (row + over, cell)),
                );
            }
        }
        let units = self.survey_units(survey, false);
        let (lines, cursor) = windowed(&units, room, self.look.glyphs);
        let mut paned = held(lines, foot, below, cursor);
        // Given fewer rows than the act's row and what stands under the survey, what stands
        // under it gives way from its end, and nothing stands in no row.
        paned.lines.truncate(rows);
        paned
    }

    /// What `survey` says over its rows and its lists, as a window takes them: the sentence,
    /// a blank row under it where `aired` says, then each list, its heading and its rows, a
    /// blank row between two lists; then the blank row over the act's row.
    fn survey_units(&self, survey: &Survey, aired: bool) -> Vec<Unit> {
        let across = self.look.width.across();
        let mut units: Vec<Unit> = Vec::new();
        if let Some(said) = &survey.said {
            units.push(Unit::said(said.rows_within(across)));
            if aired {
                units.push(Unit::blank());
            }
        }
        let labels = survey.lists.iter().flat_map(|list| &list.rows);
        let column = labels.map(|row| cell_width(&row.label)).max();
        let value_at = cell_width(INDENT) + MARK + column.unwrap_or_default() + GAP;
        for (at, list) in survey.lists.iter().enumerate() {
            if at > 0 {
                units.push(Unit::blank());
            }
            let heading = self.heading(&list.heading);
            let heading = heading.into_iter().filter(|line| !line.text().is_empty());
            units.push(Unit::said(heading.collect()));
            units.extend(list.rows.iter().map(|row| self.survey_row(row, value_at)));
        }
        units.push(Unit::blank());
        units
    }

    /// The act's row of `survey`, cut at the edge.
    fn survey_foot(&self, survey: &Survey) -> Line {
        survey.foot.cut(self.look.width.cells(), self.look.glyphs)
    }

    /// One row of a survey, what stands for it at `value_at` cells in, and the line typed in
    /// place under it where one is.
    fn survey_row(&self, row: &SurveyRow, value_at: usize) -> Unit {
        let glyphs = self.look.glyphs;
        let margin = Line::read_along(row.pointed, glyphs);
        let head = margin
            .push(Tone::of(row.mark), glyphs.mark(row.mark).to_string())
            .plain(" ")
            .push(Tone::Strong, row.label.as_str());
        let mut lines = self.valued(head, &row.value, value_at);
        let mut cursor = None;
        if let Some(typed) = &row.typed {
            let placed = self.typed_in_place(typed, value_at);
            let opens = lines.len();
            cursor = placed.cursor.map(|(line, cell)| (opens + line, cell));
            lines.extend(placed.lines);
        }
        Unit {
            lines,
            counted: true,
            blank: false,
            pointed: row.pointed,
            cursor,
        }
    }

    /// A row's `head`, up to its label, with `value` after it `value_at` cells in: words found
    /// going on under themselves, a composed row cut at the edge, or a row of words with its
    /// chosen word there, its pointer before it.
    fn valued(&self, head: Line, value: &SurveyValue, value_at: usize) -> Vec<Line> {
        let (glyphs, cells) = (self.look.glyphs, self.look.width.cells());
        let taken = cell_width(&head.text());
        let pad = |to: usize| " ".repeat(to.saturating_sub(taken));
        match value {
            SurveyValue::Found(found) => {
                let mut parts = wrap(found, cells.saturating_sub(value_at)).into_iter();
                let first = head
                    .plain(pad(value_at))
                    .plain(parts.next().unwrap_or_default());
                let hang = " ".repeat(value_at);
                std::iter::once(first)
                    .chain(parts.map(|part| Line::new().plain(hang.as_str()).plain(part)))
                    .collect()
            }
            SurveyValue::Composed(composed) => {
                let line = head.plain(pad(value_at)).then(composed.clone());
                vec![line.cut(cells, glyphs)]
            }
            SurveyValue::Chosen(choice) => {
                let start = value_at.saturating_sub(CHOSEN_LEAD).max(taken);
                let on = self.at_width(crate::look::Width::exact(cells.saturating_sub(start)));
                let words = on.word_choice(choice).unwrap_or_default();
                vec![head.plain(pad(start)).then(words).cut(cells, glyphs)]
            }
        }
    }

    /// A line typed in place `at` cells in: its box, its label in the top edge, as wide as it
    /// asks and the row leaves; the hint muted under it, going on under itself; the cursor in
    /// the box where the keys type in it.
    fn typed_in_place(&self, typed: &TypedInPlace, at: usize) -> Placed {
        let cells = self.look.width.cells();
        let room = cells.saturating_sub(at);
        let boxed = typed
            .typed
            .drawn_titled(&typed.label, typed.cells.min(room), self.look.glyphs);
        let margin = " ".repeat(at);
        let mut lines: Vec<Line> = boxed
            .lines
            .into_iter()
            .map(|line| Line::new().plain(margin.as_str()).then(line))
            .collect();
        let hint = typed.hint.iter().flat_map(|hint| wrap(hint, room));
        lines.extend(hint.map(|part| Line::new().plain(margin.as_str()).push(Tone::Muted, part)));
        let cursor = boxed.cursor.filter(|_| typed.focused);
        Placed {
            lines,
            cursor: cursor.map(|(line, cell)| (line, cell + at)),
            laid: None,
        }
    }
}

/// `units`' lines one after another, and where the cursor stands among them.
fn flat(units: &[Unit]) -> (Vec<Line>, Option<(usize, usize)>) {
    let mut lines = Vec::new();
    let mut cursor = None;
    for unit in units {
        if let Some((line, cell)) = unit.cursor {
            cursor = Some((lines.len() + line, cell));
        }
        lines.extend(unit.lines.iter().cloned());
    }
    (lines, cursor)
}

/// `lines` over the act's row `foot`, then `below`, and the cursor where one stands.
fn held(
    mut lines: Vec<Line>,
    foot: Line,
    below: Vec<Line>,
    cursor: Option<(usize, usize)>,
) -> Paned {
    lines.push(foot);
    lines.extend(below);
    Paned {
        lines,
        cursor,
        laid: Laid::default(),
    }
}

/// `units` in a window `rows` high, its last row the blank one over the act's row: reaching
/// down to that row where it keeps the row pointed at in sight, else opening at that row; the
/// rows it hides counted over it and under it, a window never opening on a blank row. A row
/// pointed at taller than what the counts leave it gives way from its end, down to its own row;
/// under that, the counts give way, the one under first. Never more lines than `rows`, and the
/// cursor only on a line it keeps.
fn windowed(
    units: &[Unit],
    rows: usize,
    glyphs: crate::glyphs::Glyphs,
) -> (Vec<Line>, Option<(usize, usize)>) {
    if rows == 0 {
        return (Vec::new(), None);
    }
    let Window {
        first,
        end,
        pointed,
    } = Window::over(units, rows);
    let counted = |range: std::ops::Range<usize>| -> usize {
        units[range].iter().filter(|unit| unit.counted).count()
    };
    let mut shown: Vec<Line> = Vec::new();
    let mut cursor = None;
    for (at, unit) in units.iter().enumerate().take(end).skip(first) {
        let keeps = match pointed {
            Some((pointed, keeps)) if pointed == at => keeps,
            _ => unit.lines.len(),
        };
        if let Some((line, cell)) = unit.cursor.filter(|(line, _)| *line < keeps) {
            cursor = Some((shown.len() + line, cell));
        }
        shown.extend(unit.lines.iter().take(keeps).cloned());
    }
    // Where the row pointed at, down to its own row, leaves fewer rows than the counts take,
    // the counts give way, the one under first.
    let mut spare = rows.saturating_sub(shown.len());
    let above = Line::hidden(counted(0..first), Hidden::Above, glyphs).filter(|_| spare > 0);
    spare -= usize::from(above.is_some());
    let under = Line::hidden(counted(end..units.len()), Hidden::Under, glyphs)
        .filter(|_| end < units.len() && spare > 0);
    let opens = usize::from(above.is_some());
    let mut lines: Vec<Line> = above.into_iter().collect();
    lines.extend(shown);
    lines.extend(under);
    (lines, cursor.map(|(row, cell)| (row + opens, cell)))
}

/// Which of a survey's units a window shows: from `first` to before `end`, and, where it opens
/// on the row pointed at, that row's place among them and how many of its lines it keeps.
struct Window {
    first: usize,
    end: usize,
    pointed: Option<(usize, usize)>,
}

impl Window {
    /// The window `rows` high over `units`: from the act's row up, as many as fit under the
    /// count of what is hidden over them, where that keeps the row pointed at; else opening on
    /// the row pointed at, as many lines of it as the counts leave, its own row at least, and as
    /// many units after it as fit. Never on a blank row.
    fn over(units: &[Unit], rows: usize) -> Window {
        let pointed = units.iter().position(|unit| unit.pointed);
        let height = |range: std::ops::Range<usize>| -> usize {
            units[range].iter().map(|unit| unit.lines.len()).sum()
        };
        let last = units.len();
        let mut first = last;
        while first > 0 && height(first - 1..last) < rows {
            first -= 1;
        }
        let (mut first, end, pointed) = match pointed {
            Some(at) if at < first => {
                // The row pointed at stands higher: the window opens on it and counts what it
                // hides under it, a row of its own, and over it where it hides any there.
                let counts = 1 + usize::from(units[..at].iter().any(|unit| unit.counted));
                let keeps = units[at]
                    .lines
                    .len()
                    .min(rows.saturating_sub(counts).max(1));
                let mut end = at + 1;
                while end < last && keeps + height(at + 1..end + 1) + counts <= rows {
                    end += 1;
                }
                (at, end, Some((at, keeps)))
            }
            _ => (first, last, None),
        };
        while first < end && units[first].blank {
            first += 1;
        }
        Window {
            first,
            end,
            pointed,
        }
    }
}
