//! Where one run's wall-clock went, a row for each share that has any.

use std::collections::BTreeMap;
use std::time::Duration;

use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_core::units::DURATION_WIDEST;
use yunta_engine::TimeSpent;

use crate::render::ink::Line;
use crate::render::{bar, bar_cells, cell_width, duration, id_column, middle_cut, Look, INDENT};

/// The block `yunta stats <run>` shows of `time`, always in the same
/// order so two runs read side by side. What a run spent on nothing is
/// what it spent measuring while nothing else ran, in guards whose
/// answer was already decided, and between steps. A share under a second
/// reads as nothing, so it is no row. Nothing for a run that spent no
/// time at all.
pub(super) fn render(time: &TimeSpent, look: &Look) -> Vec<Line> {
    let rows: Vec<(&str, Duration, String)> = shares(time)
        .into_iter()
        .filter(|(_, spent, _)| spent.as_secs() > 0)
        .collect();
    let Some(longest) = rows.iter().map(|(_, spent, _)| spent.as_secs()).max() else {
        return Vec::new();
    };
    let column = id_column(rows.iter().map(|(name, _, _)| *name));
    let cells = bar_cells(
        look.width.cells(),
        cell_width(INDENT) + column + DURATION_WIDEST + 10,
    );
    let mut lines = vec![Line::new(), Line::new().plain("time:")];
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
        lines.push(Line::new().plain(format!(
            "{INDENT}{}{bar}  {}{note}",
            middle_cut(name, column, look.glyphs),
            duration(spent),
        )));
    }
    lines
}

/// Each share with what it says beside its figure.
fn shares(time: &TimeSpent) -> [(&'static str, Duration, String); 8] {
    let decided = match time.decided_checks.is_zero() {
        true => String::new(),
        false => format!(
            "{} of it guards a red criterion had already decided",
            duration(time.decided_checks)
        ),
    };
    [
        ("working", time.working, String::new()),
        (
            "rails",
            time.rails,
            "formatting what sessions wrote, while they waited".to_string(),
        ),
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

/// The latency of each moment the rail answered, one line per moment that
/// has a sample: its median and its 95th percentile, read from the
/// histogram the run's summaries merged. A run that asked the rail nothing
/// draws no block.
pub(super) fn rail_latency(rails: &BTreeMap<RailEvent, Histogram>, look: &Look) -> Vec<Line> {
    let moments: Vec<(&str, Duration, Duration)> = rails
        .iter()
        .filter(|(_, histogram)| histogram.count > 0)
        .filter_map(|(moment, histogram)| {
            Some((
                moment.as_str(),
                histogram.percentile(0.5)?,
                histogram.percentile(0.95)?,
            ))
        })
        .collect();
    if moments.is_empty() {
        return Vec::new();
    }
    let column = id_column(moments.iter().map(|(moment, _, _)| *moment));
    let mut lines = vec![Line::new(), Line::new().plain("rails:")];
    for (moment, median, high) in moments {
        lines.push(Line::new().plain(format!(
            "{INDENT}{} p50 {} {} p95 {}",
            middle_cut(moment, column, look.glyphs),
            yunta_render::latency(median),
            look.glyphs.sep(),
            yunta_render::latency(high),
        )));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines as the text a reader sees, each on its own line.
    fn text(lines: &[Line]) -> String {
        lines
            .iter()
            .map(|line| format!("{}\n", line.text()))
            .collect()
    }

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
        let drawn = text(&render(&time, &Look::plain()));
        let rows: Vec<&str> = drawn
            .lines()
            .skip_while(|line| *line != "time:")
            .skip(1)
            .collect();

        assert_eq!(rows.len(), 3, "a share under a second is no row: {drawn}");
        assert!(
            rows[0].starts_with("  working") && rows[0].ends_with("10m00s"),
            "{drawn}"
        );
        assert!(
            rows[1].starts_with("  checks")
                && rows[1]
                    .ends_with("5m00s | 4m00s of it guards a red criterion had already decided"),
            "{drawn}"
        );
        assert!(
            rows[2].starts_with("  measuring")
                && rows[2].ends_with("2m00s | the suite, while nothing else ran"),
            "{drawn}"
        );
    }

    #[test]
    fn a_run_that_spent_no_time_has_no_block() {
        assert!(render(&TimeSpent::default(), &Look::plain()).is_empty());
    }

    #[test]
    fn a_moment_the_rail_answered_reads_its_median_and_its_95th_percentile() {
        let mut before_write = Histogram::default();
        for _ in 0..20 {
            before_write.observe(Duration::from_millis(3));
        }
        let rails = BTreeMap::from([
            (RailEvent::BeforeWrite, before_write),
            (RailEvent::AfterWrite, Histogram::default()),
        ]);

        assert_eq!(
            text(&rail_latency(&rails, &Look::plain())),
            "\nrails:\n  before-write p50 3 ms | p95 3 ms\n"
        );
    }

    #[test]
    fn a_run_that_asked_the_rail_nothing_draws_no_block() {
        assert!(rail_latency(&BTreeMap::new(), &Look::plain()).is_empty());
    }
}
