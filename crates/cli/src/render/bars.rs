//! A magnitude drawn as a shape: one value against a maximum, and a
//! series against its own peak.

use super::glyphs::Glyphs;

/// The cells a bar occupies, filled and empty together. Twenty gives
/// each step five percent of the maximum, which is as fine as an eye
/// reads a bar, and leaves the numbers beside it their share of
/// [`LINE_WIDTH`](super::LINE_WIDTH).
pub(crate) const BAR_WIDTH: usize = 20;

/// `value` against `max`, drawn in exactly [`BAR_WIDTH`] cells.
///
/// A maximum of zero is a row with nothing to compare — the bar is drawn
/// empty rather than full, because nothing measured is not everything.
pub(crate) fn bar(value: u64, max: u64, glyphs: Glyphs) -> String {
    if max == 0 {
        return repeat(glyphs.bar_empty(), BAR_WIDTH);
    }
    let filled = (((value as f64 / max as f64) * BAR_WIDTH as f64).round() as usize).min(BAR_WIDTH);
    format!(
        "{}{}",
        repeat(glyphs.bar_filled(), filled),
        repeat(glyphs.bar_empty(), BAR_WIDTH.saturating_sub(filled))
    )
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
pub(crate) fn sparkline(values: &[f64], cells: usize, glyphs: Glyphs) -> String {
    if values.is_empty() || cells == 0 {
        return String::new();
    }
    let mut out = String::new();
    let values = if values.len() > cells {
        out.push(glyphs.ellipsis());
        let from = values.len().saturating_sub(cells.saturating_sub(1));
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
    use crate::render::cell_width;

    #[test]
    fn a_bar_is_the_same_width_at_every_value() {
        for (value, max) in [(0, 0), (0, 10), (3, 10), (10, 10)] {
            for glyphs in [Glyphs::Unicode, Glyphs::Ascii] {
                assert_eq!(cell_width(&bar(value, max, glyphs)), BAR_WIDTH);
            }
        }
    }

    #[test]
    fn a_bar_with_nothing_to_compare_against_is_empty_not_full() {
        assert_eq!(bar(7, 0, Glyphs::Ascii), ".".repeat(BAR_WIDTH));
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
        assert_eq!(sparkline(&[0.0, 0.0], 10, Glyphs::Ascii), "..");
    }
}
