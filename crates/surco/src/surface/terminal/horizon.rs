//! The field in perspective on a terminal: its furrows drawn as lines of dots on a grid two
//! dots across and four down each cell, and each cell as the braille cell of its dots.
//!
//! The field is a plane seen from its foot: across, from its left side at −1 to its right
//! at 1; up, from its foot at 0 to its horizon at 1. A furrow runs from near the middle of
//! the horizon to the foot, the furrows spread wider apart there than at the horizon, so
//! they seem to meet beyond it; one that would reach the foot past a side ends where it
//! meets that side, as the field goes on past what is seen of it. A point of the plane is
//! the dot nearest it, the plane's corners on the grid's corner dots, and a furrow is every
//! dot a line between its ends passes, a step at a time along its longer axis (Bresenham's
//! line). So the furrows of a short region crowd into solid cells near the horizon, as
//! distance does.
//!
//! A cell no furrow passes is the ground: a blank, not an empty braille cell, so what
//! stands under the region shows through. A row ends after its last furrow.

use super::Terminal;
use crate::components::Horizon;
use crate::line::Line;

/// The fewest rows the field stands in: in fewer its furrows lie too flat to read as running
/// away, and the rows are better left to what the region says.
pub(super) const LEAST: usize = 3;

/// The furrows to each side of the one in the middle: nineteen in all, the middle nine
/// reaching the foot and the rest running off the sides.
const ASIDE: i32 = 9;

/// How far apart the furrows stand at the horizon, as a part of the field's half-width:
/// close, so the nineteen take under a third of its width there and seem to meet beyond it.
const APART_AT_THE_HORIZON: f64 = 0.035;

/// How far apart the furrows stand at the foot, as a part of the field's half-width: a
/// quarter, so the middle nine reach the foot and the furrows outside them run off the sides.
const APART_AT_THE_FOOT: f64 = 0.25;

/// The dots a cell holds across: a braille cell's two columns.
const DOTS_ACROSS: usize = 2;

/// The dots a cell holds down: a braille cell's four rows.
const DOTS_DOWN: usize = 4;

impl Terminal {
    /// The field `horizon` draws in `rows` rows across this look's width, a line a row, each
    /// ending after its last furrow: none in fewer rows than the least it stands in, and none
    /// where the glyphs draw no dots (ASCII), so the rows it would take stay blank.
    pub fn horizon(&self, horizon: &Horizon, rows: usize) -> Vec<Line> {
        let cells = self.look.width.cells();
        if rows < LEAST || cells == 0 {
            return Vec::new();
        }
        let mut field = Dots::new(cells, rows);
        for furrow in -ASIDE..=ASIDE {
            let at = f64::from(furrow);
            field.furrow(
                (at * APART_AT_THE_HORIZON, 1.0),
                (at * APART_AT_THE_FOOT, 0.0),
            );
        }
        let glyphs = self.look.glyphs;
        field
            .lit
            .chunks(cells)
            .map(|row| {
                let drawn = row
                    .iter()
                    .map(|lit| match lit {
                        0 => Some(' '),
                        lit => glyphs.dots(*lit),
                    })
                    .collect::<Option<String>>()?;
                let furrows = drawn.trim();
                let ground = drawn.len() - drawn.trim_start().len();
                Some(
                    Line::new()
                        .plain(" ".repeat(ground))
                        .push(horizon.tone, furrows),
                )
            })
            .collect::<Option<Vec<Line>>>()
            .unwrap_or_default()
    }
}

/// The dots of a region, two across and four down each of its cells, with the field's plane
/// over them.
struct Dots {
    /// The region's cells across.
    cells: usize,
    /// Each cell's lit dots, a row of cells after another: the dot `across, down` of a cell
    /// is its bit `across + 2 * down`.
    lit: Vec<u8>,
}

impl Dots {
    fn new(cells: usize, rows: usize) -> Self {
        Dots {
            cells,
            lit: vec![0; cells * rows],
        }
    }

    /// The dot nearest `(x, y)`, a point of the plane, as its place across and down.
    fn dot(&self, (x, y): (f64, f64)) -> (usize, usize) {
        let across = (self.cells * DOTS_ACROSS) as f64;
        let down = (self.lit.len() / self.cells * DOTS_DOWN) as f64;
        // No point of the field lies off the plane, so neither place is under zero.
        (
            ((x + 1.0) * (across - 1.0) / 2.0).round() as usize,
            ((1.0 - y) * (down - 1.0)).round() as usize,
        )
    }

    /// Lights every dot of a furrow from `top`, on the horizon, to `foot`, at the field's
    /// foot, or where the furrow meets a side before it.
    fn furrow(&mut self, top: (f64, f64), foot: (f64, f64)) {
        let foot = match foot.0.abs() > 1.0 {
            true => {
                let side = foot.0.signum();
                let met = foot.1 + (side - foot.0) * (top.1 - foot.1) / (top.0 - foot.0);
                (side, met)
            }
            false => foot,
        };
        let (from, to) = (self.dot(top), self.dot(foot));
        each_dot(from, to, |across, down| self.light(across, down));
    }

    /// Lights the dot `across, down` in the cell that holds it.
    fn light(&mut self, across: usize, down: usize) {
        let cell = down / DOTS_DOWN * self.cells + across / DOTS_ACROSS;
        if let Some(lit) = self.lit.get_mut(cell) {
            *lit |= 1 << (across % DOTS_ACROSS + DOTS_ACROSS * (down % DOTS_DOWN));
        }
    }
}

/// Each dot a line from `from` to `to` passes, as `(across, down)`: one at each step along
/// its longer axis, with a step along the shorter one where the line has gone past half a
/// dot from the last. A line as long down as across steps down.
fn each_dot(from: (usize, usize), to: (usize, usize), mut light: impl FnMut(usize, usize)) {
    let steep = to.1.abs_diff(from.1) >= to.0.abs_diff(from.0);
    // Along the longer axis first, from its lower end.
    let along = |(across, down): (usize, usize)| match steep {
        true => (down, across),
        false => (across, down),
    };
    let (start, end) = match along(from).0 <= along(to).0 {
        true => (along(from), along(to)),
        false => (along(to), along(from)),
    };
    let long = (end.0 - start.0) as isize;
    let short = end.1.abs_diff(start.1) as isize;
    let mut past = 2 * short - long;
    let mut aside = start.1;
    for step in start.0..=end.0 {
        match steep {
            true => light(aside, step),
            false => light(step, aside),
        }
        if past > 0 {
            // A step after the last dot can pass the line's end, at the grid's edge: nothing
            // is lit there.
            aside = match start.1 > end.1 {
                true => aside.saturating_sub(1),
                false => aside + 1,
            };
            past -= 2 * long;
        }
        past += 2 * short;
    }
}

#[cfg(test)]
mod tests;
