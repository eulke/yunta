//! Where one run's wall-clock went, a row for each share that has any.

use std::collections::BTreeMap;
use std::time::Duration;

use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
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
pub(super) fn rail_latency(rails: &BTreeMap<RailEvent, Histogram>, look: &Look) -> String {
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
        return String::new();
    }
    let column = id_column(moments.iter().map(|(moment, _, _)| *moment));
    let mut out = String::from("\nrails:\n");
    for (moment, median, high) in moments {
        out.push_str(&format!(
            "{INDENT}{} p50 {} {} p95 {}\n",
            middle_cut(moment, column, look.glyphs),
            latency(median),
            look.glyphs.sep(),
            latency(high),
        ));
    }
    out
}

/// A latency as a person reads it: whole milliseconds under a second, and
/// the duration helper's words from a second up, since that helper counts
/// in whole seconds and a rail call takes milliseconds.
fn latency(took: Duration) -> String {
    match (took.as_secs(), took.as_millis()) {
        (0, 0) => "under 1 ms".to_string(),
        (0, millis) => format!("{millis} ms"),
        _ => duration(took),
    }
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
            rail_latency(&rails, &Look::plain()),
            "\nrails:\n  before-write p50 3 ms | p95 3 ms\n"
        );
    }

    #[test]
    fn a_run_that_asked_the_rail_nothing_draws_no_block() {
        assert_eq!(rail_latency(&BTreeMap::new(), &Look::plain()), "");
    }

    #[test]
    fn a_latency_reads_in_milliseconds_under_a_second() {
        assert_eq!(latency(Duration::from_micros(400)), "under 1 ms");
        assert_eq!(latency(Duration::from_millis(12)), "12 ms");
        assert_eq!(latency(Duration::from_millis(2_500)), "2s");
    }
}
