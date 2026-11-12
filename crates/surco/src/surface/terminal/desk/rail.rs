//! The rail of a plan's desk: the places a person can stand on, and what the plan says of
//! itself as a whole. What gives way on a short terminal is decided here, a group at a time.

use crate::components::{RailGroup, RailRow};
use crate::diagram::outline::outline;
use crate::diagram::Diagram;
use crate::glyphs::{Glyphs, Mark};
use crate::line::{Fill, Line, Tone};
use crate::look::Look;
use crate::surface::terminal::APART;
use crate::width::{cell_width, middle_cut};

/// The fewest cells a step's number takes: one digit and a space, and a row with no number keeps
/// them. A group with a wider number gives every row of it what that number needs.
const LEAD: usize = 2;
/// The cells the pointer and the space after it take: a row not pointed at keeps them, so no
/// row moves when the pointer does.
const POINTER: usize = 2;
/// The cells a mark and the space after it take: what wraps stands under it.
const MARKED: usize = 2;
/// The cells a tally's count is right-aligned in: counts up to 99 stand in one column.
const COUNT: usize = 2;
/// The cells a tally's key takes: two spaces and the key, kept where it has no key so every
/// count stands in one column.
const KEY: usize = 3;

/// The rail's rows in `height` rows, each laid out in `cells`: its groups in order, a blank
/// row between two of them, and a window around the row pointed at when they are many.
pub(super) fn rail_rows(
    groups: &[RailGroup],
    look: &Look,
    cells: usize,
    height: usize,
) -> Vec<Line> {
    let glyphs = look.glyphs;
    let drawn: Vec<Vec<Line>> = groups
        .iter()
        .map(|group| group_lines(group, glyphs, cells))
        .collect();
    let mut lines = window(kept_lines(groups, &drawn, height), height);
    lines.resize(height, Line::new());
    lines.iter().map(|line| fit(line, cells, glyphs)).collect()
}

/// The rows the rail needs to keep every group, laid out in `cells`: each group's rows and
/// the blank row after it, as [`kept_lines`] counts what a group costs.
pub(super) fn rail_length(groups: &[RailGroup], look: &Look, cells: usize) -> usize {
    groups
        .iter()
        .map(|group| group_lines(group, look.glyphs, cells).len() + 1)
        .sum()
}

/// The lines of the groups the rail keeps, with a blank row between two of them. A group with
/// a place to stand on is always kept. The others are kept in order while each fits in what
/// the places leave, and at the first that does not fit, it and every one after it go.
fn kept_lines(groups: &[RailGroup], drawn: &[Vec<Line>], height: usize) -> Vec<Line> {
    let stops: usize = groups
        .iter()
        .zip(drawn)
        .filter(|(group, _)| has_stop(group))
        .map(|(_, rows)| rows.len() + 1)
        .sum();
    let mut left = height.saturating_sub(stops);
    let mut open = true;
    let mut lines: Vec<Line> = Vec::new();
    for (group, rows) in groups.iter().zip(drawn) {
        // A group takes its rows and the blank row after it.
        let cost = rows.len() + 1;
        let keep = has_stop(group) || {
            open = open && cost <= left;
            if open {
                left -= cost;
            }
            open
        };
        if !keep || rows.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(Line::new());
        }
        lines.extend(rows.iter().cloned());
    }
    lines
}

/// The `height` lines of a rail too long for its place, around the line a pointer stands on.
fn window(lines: Vec<Line>, height: usize) -> Vec<Line> {
    if lines.len() <= height {
        return lines;
    }
    let pointed = lines
        .iter()
        .position(|line| line.fill() == Some(Fill::Selection))
        .unwrap_or(0);
    let start = pointed.saturating_sub(height / 2).min(lines.len() - height);
    lines.into_iter().skip(start).take(height).collect()
}

/// Whether a group holds a place to stand on, which keeps it whatever room the rail has.
fn has_stop(group: &RailGroup) -> bool {
    group
        .rows
        .iter()
        .any(|row| matches!(row, RailRow::Stop { .. }))
}

/// A group's lines in `cells`: its title in strong when it has one, then each of its rows.
/// The pairs of a group share the widest far end among them, at most half the row.
fn group_lines(group: &RailGroup, glyphs: Glyphs, cells: usize) -> Vec<Line> {
    let far = group
        .rows
        .iter()
        .filter_map(|row| match row {
            RailRow::Pair { right, .. } => Some(cell_width(right)),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .min(cells / 2);
    let lead = group
        .rows
        .iter()
        .filter_map(|row| match row {
            RailRow::Stop { lead, .. } => Some(cell_width(lead) + 1),
            _ => None,
        })
        .fold(LEAD, usize::max);
    let title = group.title.iter().map(|title| {
        Line::new()
            .push(Tone::Strong, title.as_str())
            .cut(cells, glyphs)
    });
    title
        .chain(
            group
                .rows
                .iter()
                .flat_map(|row| row_lines(row, (lead, far), glyphs, cells)),
        )
        .collect()
}

/// One rail row laid out in `cells`: a stop, a tally, what weighs, a pair, or an order.
fn row_lines(
    row: &RailRow,
    (lead_cells, far): (usize, usize),
    glyphs: Glyphs,
    cells: usize,
) -> Vec<Line> {
    match row {
        RailRow::Stop {
            lead,
            name,
            pointed,
        } => {
            let lead = format!("{lead:<lead_cells$}");
            vec![stop_line(&lead, name, *pointed, glyphs, cells)]
        }
        RailRow::Tally { what, count, key } => {
            vec![tally_line(what, *count, *key, glyphs, cells)]
        }
        RailRow::Weighs(words) => weighs_lines(words, glyphs, cells),
        RailRow::Pair { left, right } => vec![pair_line(left, right, far, glyphs, cells)],
        RailRow::Order(diagram) => order_lines(diagram, glyphs, cells),
    }
}

/// A stop: its step's number in the cells its group gives it, the pointer when it is the one
/// pointed at, and its name. The pointed row is set apart by a fill, and every row is padded to
/// the same cells.
fn stop_line(lead: &str, name: &str, pointed: bool, glyphs: Glyphs, cells: usize) -> Line {
    let pointer = match pointed {
        true => Line::new()
            .push(Tone::NeedsYou, glyphs.pointer().to_string())
            .plain(" "),
        false => Line::new().plain(" ".repeat(POINTER)),
    };
    let name_tone = match pointed {
        true => Tone::Strong,
        false => Tone::Plain,
    };
    let row = Line::new().plain(lead).then(pointer).push(name_tone, name);
    let row = fit(&row, cells, glyphs);
    match pointed {
        true => row.filled(Fill::Selection),
        false => row,
    }
}

/// A tally: what is counted, its count right-aligned, and its key at the far end of the row,
/// so the key stays in sight whatever the count is.
fn tally_line(what: &str, count: usize, key: Option<char>, glyphs: Glyphs, cells: usize) -> Line {
    let count = format!("{count:>width$}", width = COUNT);
    let key = match key {
        Some(key) => format!("  {key}"),
        None => " ".repeat(KEY),
    };
    let beside = cell_width(&count) + KEY;
    let what = fit(
        &Line::new().plain(what),
        cells.saturating_sub(beside),
        glyphs,
    );
    what.push(Tone::Strong, count).push(Tone::Muted, key)
}

/// What weighs on the decision: a caution mark, then its words, wrapped under the mark.
fn weighs_lines(words: &str, glyphs: Glyphs, cells: usize) -> Vec<Line> {
    let mark = Line::new()
        .push(Tone::Caution, glyphs.mark(Mark::Caution).to_string())
        .plain(" ");
    let parts = Line::new()
        .push(Tone::Plain, words)
        .wrap(cells.saturating_sub(MARKED));
    hang(parts, mark)
}

/// A pair: its left part cut in the middle where it is too long, and its right part at the
/// far end, `far` cells wide.
fn pair_line(left: &str, right: &str, far: usize, glyphs: Glyphs, cells: usize) -> Line {
    let left_cells = cells.saturating_sub(APART + far);
    let right = match cell_width(right) > far {
        true => middle_cut(right, far, glyphs),
        false => format!("{}{right}", " ".repeat(far - cell_width(right))),
    };
    Line::new()
        .push(Tone::Plain, middle_cut(left, left_cells, glyphs))
        .plain(" ".repeat(APART))
        .push(Tone::Muted, right)
}

/// A diagram read as one line per chain of its boxes, each wrapped to `cells`. Nothing for a
/// diagram that does not read as a flowchart.
fn order_lines(diagram: &Diagram, glyphs: Glyphs, cells: usize) -> Vec<Line> {
    let Some(chart) = &diagram.chart else {
        return Vec::new();
    };
    outline(chart, glyphs)
        .iter()
        .flat_map(|row| {
            let parts = Line::new().push(Tone::Plain, row.as_str()).wrap(cells);
            hang(parts, Line::new())
        })
        .collect()
}

/// The parts of a line that wraps: the first after `first`, and each later one under the
/// cells a mark takes, as the words of a mark stand under it.
fn hang(parts: Vec<Line>, first: Line) -> Vec<Line> {
    let margin = " ".repeat(MARKED);
    parts
        .into_iter()
        .enumerate()
        .map(|(at, part)| match at {
            0 => first.clone().then(part),
            _ => part.under(&margin),
        })
        .collect()
}

/// `line` cut to `cells` and padded with spaces to them, so a row of the rail takes the same
/// cells whatever it says.
fn fit(line: &Line, cells: usize, glyphs: Glyphs) -> Line {
    let cut = line.cut(cells, glyphs);
    let spare = cells.saturating_sub(cell_width(&cut.text()));
    cut.plain(" ".repeat(spare))
}

#[cfg(test)]
mod tests;
