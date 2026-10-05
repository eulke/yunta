//! Where one run's wall-clock went, a row for each share that has any.

use std::time::Duration;

use yunta_core::units::DURATION_WIDEST;
use yunta_engine::TimeSpent;

use crate::render::{bar, bar_cells, cell_width, duration, id_column, middle_cut, Look, INDENT};

/// The block `yunta stats <run>` shows of `time`, always in the same
/// order so two runs read side by side. What a run spent on nothing is
/// what it spent measuring while nothing else ran, in guards whose
/// answer was already decided, and between steps. A share under a second
/// reads as nothing, so it is no row.
pub(super) fn render(time: &TimeSpent, look: &Look) -> String {
    let rows: Vec<(&str, Duration, String)> = shares(time)
        .into_iter()
        .filter(|(_, spent, _)| spent.as_secs() > 0)
        .collect();
    let Some(longest) = rows.iter().map(|(_, spent, _)| spent.as_secs()).max() else {
        return String::new();
    };
    let column = id_column(rows.iter().map(|(name, _, _)| *name));
    let cells = bar_cells(
        look.width.cells(),
        cell_width(INDENT) + column + DURATION_WIDEST + 10,
    );
    let mut out = String::from("\ntime:\n");
    for (name, spent, note) in rows {
        let bar = bar(spent.as_secs(), longest, cells, look.glyphs);
        let bar = if bar.is_empty() {
            bar
        } else {
            format!(" {bar}")
        };
        let note = if note.is_empty() {
            note
        } else {
            format!(" {} {note}", look.glyphs.sep())
        };
        out.push_str(&format!(
            "{INDENT}{}{bar}  {}{note}\n",
            middle_cut(name, column, look.glyphs),
            duration(spent),
        ));
    }
    out
}

/// Each share with what it says beside its figure.
fn shares(time: &TimeSpent) -> [(&'static str, Duration, String); 7] {
    let decided = match time.decided_checks.is_zero() {
        true => String::new(),
        false => format!(
            "{} of it guards a red criterion had already decided",
            duration(time.decided_checks)
        ),
    };
    [
        ("working", time.working, String::new()),
        ("checks", time.checks, decided),
        (
            "measuring",
            time.measuring,
            "the suite, while nothing else ran".to_string(),
        ),
        ("people", time.people, "a person deciding".to_string()),
        (
            "offline",
            time.offline,
            "sessions waiting for their service to answer".to_string(),
        ),
        ("parked", time.parked, String::new()),
        (
            "between",
            time.between,
            "the engine between steps".to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One row a share, in one order, with what of it was spent on
    /// nothing said beside the share it is part of.
    #[test]
    fn the_time_a_run_spent_reads_by_share() {
        let time = TimeSpent {
            working: Duration::from_secs(600),
            checks: Duration::from_secs(300),
            decided_checks: Duration::from_secs(240),
            measuring: Duration::from_secs(120),
            people: Duration::from_millis(400),
            ..Default::default()
        };
        let text = render(&time, &Look::plain());
        let rows: Vec<&str> = text
            .lines()
            .skip_while(|line| *line != "time:")
            .skip(1)
            .collect();

        assert_eq!(rows.len(), 3, "a share under a second is no row: {text}");
        assert!(
            rows[0].starts_with("  working") && rows[0].ends_with("10m00s"),
            "{text}"
        );
        assert!(
            rows[1].starts_with("  checks")
                && rows[1]
                    .ends_with("5m00s | 4m00s of it guards a red criterion had already decided"),
            "{text}"
        );
        assert!(
            rows[2].starts_with("  measuring")
                && rows[2].ends_with("2m00s | the suite, while nothing else ran"),
            "{text}"
        );
    }

    #[test]
    fn a_run_that_spent_no_time_has_no_block() {
        assert_eq!(render(&TimeSpent::default(), &Look::plain()), "");
    }
}
