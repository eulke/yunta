//! See [`super`]. Rows of names with a bar each, of a value against the most of them, and a
//! figure after it: who spent a run's tokens, and a workflow's modes by their median task cost.

use surco::bars::fine_bar;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::width::{cell_width, middle_cut, LABEL_WIDTH};

/// The cells a role's name takes in who spent it: the widest label a column
/// holds, [`LABEL_WIDTH`], and a gap of two before its bar.
const ROLE_COLUMN: usize = LABEL_WIDTH + 2;

/// The cells a role's bar is drawn in, against the role that spent the
/// most: twenty-eight, so the name's column, the bar's track and the widest
/// figure of tokens stand on the line of the least terminal, 58 cells
/// inside its margins.
const SPENT_BAR: usize = 28;

/// The cells of the track a role's bar stands on: the bar's, and a gap of
/// two before its tokens, so every row's tokens start in one column.
const SPENT_TRACK: usize = SPENT_BAR + 2;

/// `rows`, each a name, its value and its figure, a row each in their order: the name in a
/// column, a fine bar of the value against the most of them in the running tone on a track of
/// fixed cells, then the figure, muted. A line too narrow for the track, or rows whose values
/// are all none, draw no bar. Who
/// spent a run's tokens, and a workflow's modes by their median task cost.
pub(super) fn barred(rows: Vec<(String, f64, String)>, look: &Look) -> Vec<Line> {
    let most = rows.iter().map(|(_, value, _)| *value).fold(0.0, f64::max);
    let widest = rows.iter().map(|(_, _, figure)| cell_width(figure)).max();
    let room = ROLE_COLUMN + SPENT_TRACK + widest.unwrap_or(0) <= look.width.cells();
    let fits = room && most > 0.0;
    let gap = " ".repeat(ROLE_COLUMN - LABEL_WIDTH);
    let lines = rows.into_iter().map(|(name, value, figure)| {
        let line = Line::new().plain(middle_cut(&name, LABEL_WIDTH, look.glyphs) + &gap);
        let line = match fits {
            true => {
                let drawn = fine_bar(value, most, SPENT_BAR, look.glyphs).drawn();
                let pad = SPENT_TRACK.saturating_sub(cell_width(&drawn));
                line.push(Tone::Running, drawn).plain(" ".repeat(pad))
            }
            false => line,
        };
        line.push(Tone::Muted, figure)
    });
    lines.collect()
}
