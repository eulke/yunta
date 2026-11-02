//! A magnitude drawn as a shape: one value against a scale in eighths of a
//! cell, parts of a whole side by side, and a series against its own peak.

use crate::glyphs::Glyphs;
use crate::width::cell_width;

/// The cells a bar occupies, filled and empty together. Twenty gives
/// each step five percent of the maximum, which is as fine as an eye
/// reads a bar, and leaves the numbers beside it their share of
/// [`LINE_WIDTH`](crate::width::LINE_WIDTH).
pub const BAR_WIDTH: usize = 20;

/// The fewest cells a bar is drawn in: fewer are too few steps for an eye
/// to compare two bars by. A quarter of [`BAR_WIDTH`].
pub const BAR_LEAST: usize = BAR_WIDTH / 4;

/// The cells a bar gets on a line of `width` cells whose other columns
/// take `taken`: what is left, up to [`BAR_WIDTH`], and none at all when
/// what is left is too few steps for an eye to compare two bars by.
pub fn bar_cells(width: usize, taken: usize) -> usize {
    match width.saturating_sub(taken) {
        left if left < BAR_LEAST => 0,
        left => left.min(BAR_WIDTH),
    }
}

/// A value drawn against a scale in a fine bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FineBar {
    /// The value fits its scale: whole cells and the step of the cell it
    /// ends in, as many cells as it measures, with no track after them.
    Measured(String),
    /// The value runs past its scale: the whole track, and the cut over its
    /// middle. The figure beside the bar says how far.
    Cut {
        before: String,
        cut: &'static str,
        after: String,
    },
}

/// `value` against `scale`, which measures `cells` cells, in the steps a
/// fine bar climbs: an eighth of a cell in Unicode, a whole cell in ASCII,
/// rounded to the nearest. A value past the scale is cut. A value of zero
/// or less, or no cells, draws nothing, and a value against a scale of zero
/// or less, or a scale that is not a number, is past it; `cells` is zero or
/// two and more, since a cut takes two.
pub fn fine_bar(value: f64, scale: f64, cells: usize, glyphs: Glyphs) -> FineBar {
    if value <= 0.0 || cells == 0 {
        return FineBar::Measured(String::new());
    }
    if value > scale.max(0.0) {
        let cut = glyphs.cut();
        let room = cells.saturating_sub(cell_width(cut));
        let before = room / 2;
        return FineBar::Cut {
            before: repeat(glyphs.bar_empty(), before),
            cut,
            after: repeat(glyphs.bar_empty(), room - before),
        };
    }
    let steps = glyphs.fine_steps();
    let per_cell = steps.len();
    let total = (value / scale * cells as f64 * per_cell as f64).round() as usize;
    let whole = steps
        .last()
        .map_or_else(String::new, |&full| repeat(full, total / per_cell));
    let part = (total % per_cell)
        .checked_sub(1)
        .and_then(|step| steps.get(step));
    FineBar::Measured(whole + &part.map(char::to_string).unwrap_or_default())
}

/// The cells of a bar of `parts`, side by side in the order given: each
/// part as many of `cells` as its share of their sum, apportioned by the
/// largest remainder so they add up to `cells`, the earlier part first on a
/// tie. A part of zero, or one too small to win a cell, gets none; parts
/// that sum to nothing get none at all.
pub fn parts(parts: &[f64], cells: usize) -> Vec<usize> {
    let sum: f64 = parts.iter().filter(|&&part| part > 0.0).sum();
    if sum <= 0.0 || cells == 0 {
        return vec![0; parts.len()];
    }
    let exact: Vec<f64> = parts
        .iter()
        .map(|&part| part.max(0.0) / sum * cells as f64)
        .collect();
    let mut given: Vec<usize> = exact.iter().map(|share| share.floor() as usize).collect();
    let mut left = cells.saturating_sub(given.iter().sum());
    let mut order: Vec<usize> = (0..parts.len()).collect();
    let remainder = |at: usize| exact.get(at).map_or(0.0, |share| share - share.floor());
    order.sort_by(|&one, &other| remainder(other).total_cmp(&remainder(one)));
    for at in order {
        if left == 0 {
            break;
        }
        if let Some(cell) = given.get_mut(at).filter(|_| remainder(at) > 0.0) {
            *cell += 1;
            left -= 1;
        }
    }
    given
}

/// `values` as one cell each, oldest first, scaled to the largest of
/// them — a shape, never a scale, so the line that carries it names the
/// value a reader needs a number for.
///
/// More values than `cells` is the ordinary case for a workflow with a
/// history: the newest fit and the line opens with the same mark a
/// truncated label carries, so the window is visible rather than passing
/// for the whole history. A window whose own peak is at or below zero is
/// drawn in the empty step throughout: there is nothing to scale
/// against, and a flat line of the lowest step would claim there is.
pub fn sparkline(values: &[f64], cells: usize, glyphs: Glyphs) -> String {
    if values.is_empty() || cells == 0 {
        return String::new();
    }
    let mut out = String::new();
    let values = if values.len() > cells {
        let mark = glyphs.ellipsis();
        out.push_str(mark);
        let from = values
            .len()
            .saturating_sub(cells.saturating_sub(crate::width::cell_width(mark)));
        values.get(from..).unwrap_or(values)
    } else {
        values
    };
    let max = values.iter().cloned().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        out.push_str(&repeat(glyphs.bar_empty(), values.len()));
        return out;
    }
    let ramp = glyphs.ramp();
    let top = ramp.len().saturating_sub(1);
    for value in values {
        let step = ((value / max) * top as f64).round() as usize;
        out.push(*ramp.get(step.min(top)).unwrap_or(&' '));
    }
    out
}

fn repeat(glyph: char, times: usize) -> String {
    glyph.to_string().repeat(times)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::width::cell_width;

    #[test]
    fn a_bar_takes_what_the_line_leaves_and_none_when_too_little_is_left() {
        assert_eq!(bar_cells(120, 60), BAR_WIDTH);
        assert_eq!(bar_cells(80, 70), 10);
        assert_eq!(bar_cells(60, 70), 0);
    }

    #[test]
    fn a_sparkline_keeps_the_newest_values_and_marks_the_window() {
        let values: Vec<f64> = (1..=40).map(f64::from).collect();
        let line = sparkline(&values, 10, Glyphs::Ascii);
        assert_eq!(cell_width(&line), 10);
        assert!(
            line.starts_with(Glyphs::Ascii.ellipsis()),
            "a windowed sparkline says so: {line}"
        );
        // The newest value is the peak, so the last cell is the top step.
        assert!(line.ends_with('#'), "got: {line}");
    }

    #[test]
    fn a_sparkline_that_fits_carries_no_window_mark() {
        let line = sparkline(&[1.0, 2.0, 4.0], 10, Glyphs::Ascii);
        assert_eq!(cell_width(&line), 3);
        assert!(line.ends_with('#'), "got: {line}");
    }

    #[test]
    fn a_series_with_no_peak_draws_the_empty_step() {
        assert_eq!(sparkline(&[0.0, 0.0], 10, Glyphs::Ascii), "__");
    }

    /// A fine bar ends in the eighth of a cell it reaches: 91 against a scale
    /// of 1465 over 20 cells is a cell and a quarter in Unicode, and the one
    /// whole cell it rounds to in ASCII.
    #[test]
    fn a_fine_bar_ends_in_the_eighth_it_reaches() {
        assert_eq!(
            fine_bar(91.0, 1465.0, 20, Glyphs::Unicode),
            FineBar::Measured("█▎".to_string())
        );
        assert_eq!(
            fine_bar(91.0, 1465.0, 20, Glyphs::Ascii),
            FineBar::Measured("#".to_string())
        );
        assert_eq!(
            fine_bar(1465.0, 1465.0, 20, Glyphs::Unicode),
            FineBar::Measured("█".repeat(20))
        );
        assert_eq!(
            fine_bar(0.0, 1465.0, 20, Glyphs::Unicode),
            FineBar::Measured(String::new())
        );
    }

    /// A value past its scale is the whole track with the cut in its middle,
    /// however little it runs past.
    #[test]
    fn a_fine_bar_past_its_scale_is_cut() {
        assert_eq!(
            fine_bar(23220.0, 1465.0, 20, Glyphs::Unicode),
            FineBar::Cut {
                before: "░".repeat(9),
                cut: "╱╱",
                after: "░".repeat(9),
            }
        );
        assert!(matches!(
            fine_bar(1466.0, 1465.0, 20, Glyphs::Unicode),
            FineBar::Cut { .. }
        ));
        let FineBar::Cut { before, cut, after } = fine_bar(23220.0, 1465.0, 20, Glyphs::Ascii)
        else {
            panic!("a value past its scale is cut");
        };
        assert_eq!(
            format!("{before}{cut}{after}"),
            format!("{}//{}", "_".repeat(9), "_".repeat(9))
        );
    }

    /// A value against a scale of zero is past it: the whole track, cut.
    #[test]
    fn a_fine_bar_against_no_scale_is_cut() {
        assert!(matches!(
            fine_bar(5.0, 0.0, 6, Glyphs::Ascii),
            FineBar::Cut { .. }
        ));
        assert_eq!(
            fine_bar(0.0, 0.0, 6, Glyphs::Ascii),
            FineBar::Measured(String::new())
        );
    }

    /// The cells a fine bar draws, whether it measured its value or was cut.
    fn drawn_width(bar: &FineBar) -> usize {
        match bar {
            FineBar::Measured(drawn) => cell_width(drawn),
            FineBar::Cut { before, cut, after } => {
                cell_width(before) + cell_width(cut) + cell_width(after)
            }
        }
    }

    /// A fine bar takes no more than the cells it is given, at every value,
    /// and a cut takes all of them. A bar that measures less is not padded
    /// here: the line it is drawn on pads it to its cells, so one width at
    /// every value is the line's job.
    #[test]
    fn a_fine_bar_takes_no_more_than_its_cells_and_a_cut_takes_all_of_them() {
        for (value, scale) in [
            (0.0, 0.0),
            (0.0, 10.0),
            (3.0, 10.0),
            (10.0, 10.0),
            (11.0, 10.0),
            (5.0, 0.0),
        ] {
            for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
                let bar = fine_bar(value, scale, BAR_WIDTH, glyphs);
                let width = drawn_width(&bar);
                assert!(
                    width <= BAR_WIDTH,
                    "{bar:?} takes {width} cells under {glyphs}"
                );
                if matches!(bar, FineBar::Cut { .. }) {
                    assert_eq!(width, BAR_WIDTH, "{bar:?} under {glyphs}");
                }
            }
        }
    }

    /// A bar of parts shares out every cell, by the largest remainder and the
    /// earlier part on a tie, and gives a part of nothing none.
    #[test]
    fn a_bar_of_parts_shares_out_every_cell() {
        assert_eq!(parts(&[3.0, 1.0], 8), [6, 2]);
        assert_eq!(parts(&[1.0, 1.0, 1.0], 10), [4, 3, 3]);
        assert_eq!(parts(&[0.0, 5.0], 4), [0, 4]);
        assert_eq!(parts(&[0.0, 0.0], 4), [0, 0]);
    }
}
