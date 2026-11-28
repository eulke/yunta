//! See [`super`]. A region that lists runs, a pointer on one of them, as the field lists its
//! runs: what stands above its lists (the row a new run starts from), the lists, a heading over
//! each, and what stands under them (the keys of the run pointed at), a blank row parting the
//! lists from what stands around them and two lists from each other. It stands a cell inside
//! each of its edges, so the row pointed at, standing on the selection to the region's edge,
//! shows a cell of it past its words at either end; and so it stands with no list yet, what
//! stands above the lists keeping its cell as the first run comes.
//!
//! Short of rows, the lists give way as a list does, what stands above and under them whole:
//! the blank rows between two lists first; then a window over the lists that keeps the row
//! pointed at in sight, opening at the earliest heading that keeps it, and counting the runs
//! it hides above and under it at the marks' column, a heading never standing at its foot
//! without a run under it. A pane standing close, as on the least terminal, keeps no blank
//! row, and there what stands above the lists gives way before the window opens. Where no
//! window keeps the row pointed at in sight beside what stands around the lists, what stands
//! above them gives way, then what stands under them, and at the last the row pointed at
//! stands alone with what counts the rest: what a key acts on never leaves sight.

use super::{closed_up, Spacing, Terminal};
use crate::components::{Laid, Runs};
use crate::doc::{Block, Doc};
use crate::line::{Hidden, Line};
use crate::look::{Look, Width};
use crate::surface::terminal::runs::LinedUp;
use crate::width::INDENT;

/// The cells a region listing runs stands inside each of its edges: one, a cell of the
/// selection either side of the words of the row pointed at.
const INSET: usize = 1;

/// A document's blocks as a region listing runs holds them: what stands above its lists, each
/// list under its heading, and what stands under them. Not [`Runs`], one list's rows.
pub(super) struct Listed<'a> {
    /// Every block of the document, which the lists line their columns up across.
    blocks: &'a [Block],
    above: &'a [Block],
    /// Each list: its heading's block and its runs' block.
    lists: Vec<(&'a Block, &'a Block)>,
    under: &'a [Block],
    /// What the region holds under its own blocks in the stead of a region beside it that
    /// gave way: the keys of the run pointed at.
    instead: &'a [Block],
}

/// A row of the lists: a heading's, or a run's, pointed at or not.
struct Row {
    line: Line,
    run: bool,
    pointed: bool,
}

impl<'a> Listed<'a> {
    /// `doc` as a region listing runs holds it: from its first heading over a list of runs,
    /// each heading and the list under it, until a block that is neither; every block above
    /// the lists where it holds none yet, as a field with no run.
    pub(super) fn of(doc: &'a Doc) -> Listed<'a> {
        let blocks = doc.blocks();
        let headed = |at: usize| {
            matches!(
                blocks.get(at..at + 2),
                Some([Block::Heading(_), Block::Runs(_)])
            )
        };
        let first = (0..blocks.len())
            .find(|at| headed(*at))
            .unwrap_or(blocks.len());
        let mut at = first;
        let mut lists = Vec::new();
        while headed(at) {
            lists.push((&blocks[at], &blocks[at + 1]));
            at += 2;
        }
        Listed {
            blocks,
            above: &blocks[..first],
            lists,
            under: &blocks[at..],
            instead: &[],
        }
    }

    /// It holding `instead` under its own blocks, in the stead of a region beside it that gave
    /// way.
    pub(super) fn holding(self, instead: &'a [Block]) -> Listed<'a> {
        Listed { instead, ..self }
    }
}

impl Terminal {
    /// `listed` in a pane `rows` high spaced by `spacing`, a cell inside each of its edges:
    /// whole where it fits, its lists apart; else the lists in a window that keeps the row
    /// pointed at in sight, close together, what stands above and under them whole, but where
    /// the pane stands close, where what stands above them gives way before the window opens.
    /// Where no window keeps the row pointed at in sight beside them, what stands above the
    /// lists gives way, then what stands under them. What was laid out of the lists comes
    /// back with the lines.
    pub(super) fn listed(
        &self,
        listed: &Listed<'_>,
        rows: usize,
        spacing: Spacing,
    ) -> (Vec<Line>, Laid) {
        let inside = self.at_width(Width::exact(
            self.look.width.cells().saturating_sub(2 * INSET),
        ));
        let aired = spacing == Spacing::Aired;
        let above = closed_up(inside.in_rhythm(listed.above), spacing);
        let under = closed_up(inside.in_rhythm(listed.under), spacing);
        let instead = closed_up(inside.in_rhythm(listed.instead), spacing);
        let under = one_after_the_other(under, instead, aired);
        let lists = inside.lists(listed);
        let listed_rows: usize = lists.iter().map(Vec::len).sum();
        let between = usize::from(aired) * lists.len().saturating_sub(1);
        let around = |above: &[Line], under: &[Line]| {
            parted(above.len(), aired) + parted(under.len(), aired)
        };
        if around(&above, &under) + listed_rows + between <= rows {
            let apart = lists.into_iter().enumerate().flat_map(|(at, list)| {
                let blank = (aired && at > 0).then(Line::new);
                blank
                    .into_iter()
                    .chain(list.into_iter().map(|row| row.line))
            });
            let lines = around_the_lists(above, apart.collect(), under, aired, rows);
            return (lines, whole(rows));
        }
        // Standing close, what stands above the lists gives way before the window opens.
        let above = match aired {
            true => above,
            false => Vec::new(),
        };
        let lists: Vec<Row> = lists.into_iter().flatten().collect();
        let around = (above, under);
        let (above, shown, under, laid) = windowed(&lists, around, rows, aired, &inside.look);
        let lines = around_the_lists(above, shown, under, aired, rows);
        (lines, laid)
    }

    /// Each of `listed`'s lists as its rows: its heading's, then a row a run, its columns
    /// lined up with every list of the document.
    fn lists(&self, listed: &Listed<'_>) -> Vec<Vec<Row>> {
        let lined = LinedUp::among(listed.blocks);
        listed
            .lists
            .iter()
            .map(|(heading, runs)| {
                // A heading drawn alone opens with the blank row that parts it from what stands
                // before it, which the lists say themselves.
                let drawn = self.block(heading).into_iter();
                let headed = drawn
                    .filter(|line| !line.text().is_empty())
                    .map(|line| Row {
                        line,
                        run: false,
                        pointed: false,
                    });
                let Block::Runs(Runs { rows }) = runs else {
                    return headed.collect();
                };
                let drawn = self.block_lined_up(runs, lined);
                let run_rows = drawn.into_iter().zip(rows).map(|(line, row)| Row {
                    line,
                    run: true,
                    pointed: row.pointed,
                });
                headed.chain(run_rows).collect()
            })
            .collect()
    }
}

/// `first`, then `then`, a blank row parting them where both stand and the pane is `aired`.
fn one_after_the_other(mut first: Vec<Line>, then: Vec<Line>, aired: bool) -> Vec<Line> {
    if aired && !first.is_empty() && !then.is_empty() {
        first.push(Line::new());
    }
    first.extend(then);
    first
}

/// The rows `count` lines take where they stand apart from the lists: with the blank row that
/// parts them where the pane is `aired`, none for no line.
fn parted(count: usize, aired: bool) -> usize {
    match count {
        0 => 0,
        count => count + usize::from(aired),
    }
}

/// What was laid out of lists shown whole in a pane `rows` high: all of them, from the first.
fn whole(rows: usize) -> Laid {
    Laid {
        top: 0,
        page: rows,
        most: 0,
    }
}

/// `lists`, the lists' rows, in a window of a pane `rows` high spaced as `aired` says, beside
/// what stands `around` them, above and under: the window that keeps the row pointed at in
/// sight beside both, else beside what stands under them alone, else beside neither, else
/// the row pointed at alone. What stands above, the rows in sight, what stands under, and
/// what was laid out of the lists.
fn windowed(
    lists: &[Row],
    around: (Vec<Line>, Vec<Line>),
    rows: usize,
    aired: bool,
    look: &Look,
) -> (Vec<Line>, Vec<Line>, Vec<Line>, Laid) {
    let (above, under) = around;
    let giving_way = [
        (above, under.clone()),
        (Vec::new(), under),
        (Vec::new(), Vec::new()),
    ];
    let opened = giving_way.into_iter().find_map(|(above, under)| {
        let budget = rows.saturating_sub(parted(above.len(), aired) + parted(under.len(), aired));
        let top = opening(lists, budget)?;
        Some((above, under, top, budget))
    });
    match opened {
        Some((above, under, top, budget)) => {
            let (shown, laid) = window(lists, top, budget, look);
            (above, shown, under, laid)
        }
        None => {
            let (shown, laid) = alone(lists, rows, look);
            (Vec::new(), shown, Vec::new(), laid)
        }
    }
}

/// `shown`, the lists' rows in sight, with what stands `above` and `under` them, each parted
/// from them by a blank row where the pane is `aired`, in at most `rows` rows, a cell inside
/// the region's edges.
fn around_the_lists(
    above: Vec<Line>,
    shown: Vec<Line>,
    under: Vec<Line>,
    aired: bool,
    rows: usize,
) -> Vec<Line> {
    let gap = || aired.then(Line::new);
    let mut lines: Vec<Line> = Vec::new();
    if !above.is_empty() {
        lines.extend(above);
        lines.extend(gap());
    }
    lines.extend(shown);
    if !under.is_empty() {
        lines.extend(gap());
        lines.extend(under);
    }
    lines.truncate(rows);
    lines.into_iter().map(inset).collect()
}

/// Where a window of `budget` rows over `rows`, the lists' rows, opens so that it keeps the
/// row pointed at in sight: at the earliest heading whose window keeps it; where none does,
/// at the earliest row under the heading over it whose window does; at the first row where
/// none is pointed at. None where no window of `budget` rows keeps it.
fn opening(rows: &[Row], budget: usize) -> Option<usize> {
    let Some(pointed) = rows.iter().position(|row| row.pointed) else {
        return Some(0);
    };
    let keeps = |top: &usize| window_of(rows, *top, budget).0.contains(&pointed);
    let headings = (0..rows.len()).filter(|at| !rows[*at].run);
    let over_it = headings.clone().rfind(|heading| *heading <= pointed);
    headings
        .clone()
        .find(keeps)
        .or_else(|| (over_it?..=pointed).find(keeps))
}

/// The window of `budget` rows over `rows`, the lists' rows, opening at `top`, drawn in
/// `look`: what it hides above it and under it counted at the marks' column, a row each;
/// and what was laid out of them.
fn window(rows: &[Row], top: usize, budget: usize, look: &Look) -> (Vec<Line>, Laid) {
    let (shown, room) = window_of(rows, top, budget);
    let runs_in = |range: std::ops::Range<usize>| rows[range].iter().filter(|row| row.run).count();
    let above = counted(runs_in(0..shown.start), Hidden::Above, look);
    let under = counted(runs_in(shown.end..rows.len()), Hidden::Under, look);
    let laid = Laid {
        top: shown.start,
        page: room,
        most: rows.len().saturating_sub(room),
    };
    let lines = above
        .into_iter()
        .chain(rows[shown].iter().map(|row| row.line.clone()))
        .chain(under)
        .collect();
    (lines, laid)
}

/// The row pointed at among `rows`, the lists' rows, alone in `budget` rows where no window
/// keeps it with what stands around it, drawn in `look`: then what it hides above it, then
/// what it hides under it, where a row stands for each; and what was laid out of them.
fn alone(rows: &[Row], budget: usize, look: &Look) -> (Vec<Line>, Laid) {
    let pointed = rows.iter().position(|row| row.pointed).unwrap_or(0);
    let runs_in = |range: std::ops::Range<usize>| rows[range].iter().filter(|row| row.run).count();
    let above = counted(runs_in(0..pointed), Hidden::Above, look).filter(|_| budget > 1);
    let under = counted(runs_in(pointed + 1..rows.len()), Hidden::Under, look);
    let under = under.filter(|_| budget > 1 + usize::from(above.is_some()));
    let laid = Laid {
        top: pointed,
        page: budget,
        most: rows.len().saturating_sub(budget),
    };
    let lines = above
        .into_iter()
        .chain(rows.get(pointed).map(|row| row.line.clone()))
        .chain(under)
        .take(budget)
        .collect();
    (lines, laid)
}

/// The row that counts the `runs` a window hides on `side`, at the marks' column; none where
/// it hides none.
fn counted(runs: usize, side: Hidden, look: &Look) -> Option<Line> {
    Line::hidden(runs, side, look.glyphs).map(|count| Line::new().plain(INDENT).then(count))
}

/// Which of `rows` a window of `budget` rows opening at `top` shows, and how many rows it
/// gives them: a row that counts what it hides above where it opens past the first, one that
/// counts what it hides under where it ends before the last, and no heading at its foot.
fn window_of(rows: &[Row], top: usize, budget: usize) -> (std::ops::Range<usize>, usize) {
    let room = budget.saturating_sub(usize::from(top > 0));
    let left = rows.len() - top;
    if left <= room {
        return (top..rows.len(), room);
    }
    let mut end = top + room.saturating_sub(1);
    while end > top && !rows[end - 1].run {
        end -= 1;
    }
    (top..end, room)
}

/// `line` a cell inside the region's edge, its fill behind that cell too; a blank line stays
/// blank.
fn inset(line: Line) -> Line {
    if line.text().is_empty() && line.fill().is_none() {
        return line;
    }
    let fill = line.fill();
    let inset = Line::new().plain(" ".repeat(INSET)).then(line);
    match fill {
        Some(fill) => inset.filled(fill),
        None => inset,
    }
}

#[cfg(test)]
mod tests;
