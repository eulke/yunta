//! A form at a terminal: what it asks over its first row and its hint under that row; each
//! other row's label over what is set on it and its description under it, a blank row between
//! two rows; then, after a blank row, its foot: its heading and, a step in, its strokes, two cells
//! a step at most as a run's row in a list draws its furrow ([`Terminal::furrow`]), and what it
//! says of them, each line going on under itself. A line typed stands in its box
//! across the width (`typed`), the cursor in the box of the row in focus and nowhere while the
//! focus stands on a row that types nothing; a list stands a row a choice, the pointer on the
//! one chosen and its names in a column (`asked`'s list); a row of words as `word_choice`
//! draws it.
//!
//! What refused the act over a row stands under the row, its words wrapped under the mark of a
//! failure that opens them (`card`'s `marked`).
//!
//! Held in fewer rows than all of it takes, it gives way step by step, a step taken only where
//! the one before still does not fit ([`GIVING_WAY`]): the blank row it opens on; the blank
//! rows between two rows that are no list, a list keeping the blank rows around it, which part
//! its rows from the rows beside it; what the rows come to, whole; every blank row; the hint.
//! Where even that does not fit, a window keeps the row in focus in sight: it opens at the
//! earliest row that lets the focused row stand and takes as many rows after it as fit, the
//! rows it hides counted above and under it in the labels' column, and a list in focus taller
//! than the window keeps its pointer in sight as a list does. The form's question stands over
//! the first row only where the window keeps every row it holds without it: where a row needs
//! its row, it gives way.
//!
//! A form whose pane holds what a region beside it acts on, where that region gave way (a new
//! run's facts that stand in the way of its run), holds it at its foot (`form_held_with`): after
//! what the rows come to, apart from them by a blank row as a list is while the form keeps
//! any, and in a window a row of its own after the last, counted with the rows it hides.

use super::asked::{listed, Columns};
use super::card::{marked, Placed};
use super::Terminal;
use crate::components::{Form, FormFoot, FormRow, FormValue};
use crate::glyphs::Mark;
use crate::line::{Hidden, Line, Tone};
use crate::width::{cell_width, wrap, INDENT};
use crate::words::Standing;

/// What a form keeps at one step of giving way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kept {
    /// The blank row it opens on.
    opening: bool,
    blanks: Blanks,
    /// What the rows come to, at its foot.
    under: bool,
    hint: bool,
}

/// Which blank rows between two rows of a form stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Blanks {
    Every,
    /// Only those beside a list.
    AroundLists,
    None,
}

/// A form drawn whole, as a document draws it: it opens on what it says, with no blank row
/// over it.
const WHOLE: Kept = Kept {
    opening: false,
    blanks: Blanks::Every,
    under: true,
    hint: true,
};

/// Each step a form held in too few rows gives way by, in order: all of it, then without the
/// blank row it opens on, the blank rows between two rows that are no list, what the rows come
/// to, every blank row, and the hint.
const GIVING_WAY: [Kept; 6] = [
    Kept {
        opening: true,
        ..WHOLE
    },
    WHOLE,
    Kept {
        blanks: Blanks::AroundLists,
        ..WHOLE
    },
    Kept {
        blanks: Blanks::AroundLists,
        under: false,
        ..WHOLE
    },
    Kept {
        blanks: Blanks::None,
        under: false,
        ..WHOLE
    },
    Kept {
        blanks: Blanks::None,
        under: false,
        hint: false,
        ..WHOLE
    },
];

/// A row of a form drawn: its lines, where the cursor stands among them while it is in focus,
/// and whether blank rows part it from the rows beside it while the form keeps any but those:
/// a list, and what the form holds at its foot for a region beside it.
struct Drawn {
    lines: Vec<Line>,
    cursor: Option<(usize, usize)>,
    apart: bool,
}

impl Terminal {
    /// `form` whole, as a document draws it.
    pub(super) fn form(&self, form: &Form) -> Vec<Line> {
        let rows: Vec<Drawn> = (0..form.rows.len())
            .map(|at| self.form_row(form, at, usize::MAX))
            .collect();
        self.form_kept(form, &rows, None, WHOLE).lines
    }

    /// `form` held in `rows` rows: all of it where it fits, else the first step of giving way
    /// that fits, else a window that keeps the row in focus in sight; the cursor in the box of
    /// the row in focus where it types and stands in sight.
    pub(super) fn form_held(&self, form: &Form, rows: usize) -> Placed {
        self.form_held_with(form, Vec::new(), rows)
    }

    /// `form` held in `rows` rows as [`Terminal::form_held`] holds it, with `stead` at its
    /// foot: what a region beside it that gave way holds for a person to act on, apart from
    /// what stands over it while the form keeps a blank row, and a row of its own in a window.
    /// None stands where `stead` holds no line.
    pub(super) fn form_held_with(&self, form: &Form, stead: Vec<Line>, rows: usize) -> Placed {
        let mut drawn: Vec<Drawn> = (0..form.rows.len())
            .map(|at| self.form_row(form, at, usize::MAX))
            .collect();
        let stead = (!stead.is_empty()).then_some(Drawn {
            lines: stead,
            cursor: None,
            apart: true,
        });
        let fitting = GIVING_WAY
            .into_iter()
            .map(|kept| self.form_kept(form, &drawn, stead.as_ref(), kept))
            .find(|placed| placed.lines.len() <= rows);
        match fitting {
            Some(placed) => placed,
            None => {
                drawn.extend(stead);
                self.form_windowed(form, drawn, rows)
            }
        }
    }

    /// `form` as it stands with what `kept` keeps of it, every row in `drawn`, and `stead`
    /// at its foot where it holds one.
    fn form_kept(&self, form: &Form, drawn: &[Drawn], stead: Option<&Drawn>, kept: Kept) -> Placed {
        let cells = self.look.width.cells();
        let glyphs = self.look.glyphs;
        let mut lines: Vec<Line> = Vec::new();
        let mut cursor = None;
        if kept.opening {
            lines.push(Line::new());
        }
        lines.extend(form.asked.iter().map(|asked| asked.cut(cells, glyphs)));
        for (at, row) in drawn.iter().enumerate() {
            let apart = match (at, kept.blanks) {
                (0, _) | (_, Blanks::None) => false,
                (_, Blanks::Every) => true,
                (_, Blanks::AroundLists) => row.apart || drawn[at - 1].apart,
            };
            if apart {
                lines.push(Line::new());
            }
            if let Some((row_at, cell)) = row.cursor {
                cursor = Some((lines.len() + row_at, cell));
            }
            lines.extend(row.lines.iter().cloned());
            if at == 0 && kept.hint {
                lines.extend(muted_wrapped(form.hint.as_deref(), cells));
            }
        }
        if let Some(foot) = form.foot.as_ref().filter(|_| kept.under) {
            if kept.blanks != Blanks::None {
                lines.push(Line::new());
            }
            lines.extend(self.form_foot(foot));
        }
        if let Some(stead) = stead {
            if kept.blanks != Blanks::None {
                lines.push(Line::new());
            }
            lines.extend(stead.lines.iter().cloned());
        }
        Placed {
            lines,
            cursor,
            laid: None,
        }
    }

    /// `form`'s rows, `drawn`, what stands at its foot for a region beside it among them, in a
    /// window of `rows` rows that keeps the row in focus in sight: from the earliest row that
    /// lets it stand, as many rows after it as fit, those it hides counted above and under it;
    /// the form's question over the first row where the window keeps every row it holds
    /// beside it.
    fn form_windowed(&self, form: &Form, mut drawn: Vec<Drawn>, rows: usize) -> Placed {
        let count = drawn.len();
        if count == 0 {
            return self.form_kept(form, &drawn, None, GIVING_WAY[GIVING_WAY.len() - 1]);
        }
        // With no row in focus, the window keeps the first in sight.
        let focus = form.focus.unwrap_or(0).min(count - 1);
        // A list in focus taller than the window keeps its pointer in sight in what the counts
        // of the rows around it leave.
        let room = rows.saturating_sub(Window::counts((focus, focus + 1), count));
        if drawn[focus].lines.len() > room {
            drawn[focus] = self.form_row(form, focus, room);
        }
        let heights: Vec<usize> = drawn.iter().map(|row| row.lines.len()).collect();
        let window = Window::keeping(&heights, focus, rows);
        let glyphs = self.look.glyphs;
        let asked = form
            .asked
            .as_ref()
            .filter(|_| window.leaves_a_row_over(&heights, rows));
        let mut lines: Vec<Line> = asked
            .map(|asked| asked.cut(self.look.width.cells(), glyphs))
            .into_iter()
            .collect();
        lines.extend(Line::hidden(window.start, Hidden::Above, glyphs));
        let mut cursor = None;
        for row in &drawn[window.start..window.end] {
            if let Some((row_at, cell)) = row.cursor {
                cursor = Some((lines.len() + row_at, cell));
            }
            lines.extend(row.lines.iter().cloned());
        }
        lines.extend(Line::hidden(count - window.end, Hidden::Under, glyphs));
        lines.truncate(rows);
        Placed {
            cursor: cursor.filter(|(row, _)| *row < lines.len()),
            lines,
            laid: None,
        }
    }

    /// `foot`, what a form's rows come to: its heading, then a step in its strokes, two cells a
    /// step at most, and what it says, each line going on under itself.
    fn form_foot(&self, foot: &FormFoot) -> Vec<Line> {
        let cells = self.look.width.cells();
        let inside = cells.saturating_sub(cell_width(INDENT));
        let standings: Vec<Standing> = foot.strokes.iter().map(|step| step.standing).collect();
        let strokes = (!standings.is_empty()).then(|| self.furrow(&standings, inside));
        let said = foot.said.iter().flat_map(|line| line.rows_within(inside));
        let stepped_in = strokes
            .into_iter()
            .chain(said)
            .map(|line| line.under(INDENT));
        std::iter::once(foot.heading.cut(cells, self.look.glyphs))
            .chain(stepped_in)
            .collect()
    }

    /// Row `at` of `form` drawn in at most `most` lines: its label cut to one row, what is set
    /// on it, its description wrapped under it, and what refused the act over it; the cursor
    /// in its box where it types and stands in focus. Only a list gives way to `most`, keeping
    /// its pointer in sight.
    fn form_row(&self, form: &Form, at: usize, most: usize) -> Drawn {
        let cells = self.look.width.cells();
        let glyphs = self.look.glyphs;
        let FormRow {
            label,
            value,
            description,
            refusal,
        } = &form.rows[at];
        let mut lines: Vec<Line> = label.iter().map(|label| label.cut(cells, glyphs)).collect();
        let mut described = muted_wrapped(description.as_deref(), cells);
        described.extend(refusal.iter().flat_map(|refusal| {
            marked(
                Mark::Failed,
                Line::new().plain(refusal.as_str()),
                cells,
                glyphs,
            )
        }));
        let mut cursor = None;
        let apart = matches!(value, FormValue::Listed(_));
        match value {
            FormValue::Typed { typed, placeholder } => {
                let boxed = typed.drawn(cells, glyphs, placeholder.as_deref());
                cursor = boxed
                    .cursor
                    .filter(|_| form.focus == Some(at))
                    .map(|(row, cell)| (lines.len() + row, cell));
                lines.extend(boxed.lines);
            }
            FormValue::Listed(choices) => {
                let budget = most.saturating_sub(lines.len() + described.len());
                let room = (cells, Columns::over(self));
                lines.extend(listed(Some(choices), &self.look, room, budget));
            }
            FormValue::Words(choice) => lines.extend(self.word_choice(choice)),
        }
        lines.extend(described);
        Drawn {
            lines,
            cursor,
            apart,
        }
    }
}

/// `text` wrapped at `cells`, each part muted on a row of its own; nothing for none.
fn muted_wrapped(text: Option<&str>, cells: usize) -> Vec<Line> {
    text.into_iter()
        .flat_map(|text| wrap(text, cells))
        .map(|part| Line::new().push(Tone::Muted, part))
        .collect()
}

/// The rows of a form a window shows, from `start` to before `end`, counted among them all.
struct Window {
    start: usize,
    end: usize,
}

impl Window {
    /// The window over rows `heights` lines high that keeps row `focus` in sight in `rows`
    /// rows: from the earliest row that lets it stand beside the counts of what it hides, as
    /// many rows after it as fit.
    fn keeping(heights: &[usize], focus: usize, rows: usize) -> Window {
        let count = heights.len();
        let used = |start: usize, end: usize| heights[start..end].iter().sum::<usize>();
        let fits = |start: usize, end: usize| {
            used(start, end) + Window::counts((start, end), count) <= rows
        };
        let start = (0..=focus)
            .find(|&start| fits(start, focus + 1))
            .unwrap_or(focus);
        let end = (focus + 1..=count)
            .take_while(|&end| fits(start, end))
            .last()
            .unwrap_or(focus + 1);
        Window { start, end }
    }

    /// The rows the counts of what a window from `start` to `end` among `count` rows hides
    /// take: one above where it hides any there, one under where it hides any there.
    fn counts((start, end): (usize, usize), count: usize) -> usize {
        usize::from(start > 0) + usize::from(end < count)
    }

    /// Whether the window leaves a row over its first, the form's first row among what it shows,
    /// in `rows` rows of rows `heights` lines high: where the form's question stands.
    fn leaves_a_row_over(&self, heights: &[usize], rows: usize) -> bool {
        let used: usize = heights[self.start..self.end].iter().sum();
        self.start == 0 && 1 + used + Window::counts((self.start, self.end), heights.len()) <= rows
    }
}
