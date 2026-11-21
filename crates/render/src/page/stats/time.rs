//! Where one run's wall-clock went: the sentence that says where most of it
//! went, and, under its heading, a bar of its parts across the line with
//! each part's mark and words under it.

use std::collections::BTreeMap;
use std::time::Duration;

use surco::bars::parts as apportioned;
use surco::figures::duration;
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::width::{id_column, middle_cut, INDENT};
use surco::words::{AT_WORK, WAITING_ON_YOU};
use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_core::NodeId;
use yunta_replay::TimeSpent;

/// The block `yunta stats <run>` shows of `time`, under its heading: a bar
/// of its parts across the line's whole width, side by side in the one
/// order two runs are read in, each part as many cells as its share and in
/// its tone, and under it each part, its mark in its tone, its name and
/// what it took muted, apart by a faint separator and wrapped where the
/// line ends. A part under a second is no part, so a run whose time holds
/// no checks, measuring or time between draws none of them. Nothing for a
/// run that spent no time at all.
pub(super) fn render(time: &TimeSpent, look: &Look) -> Vec<Line> {
    let parts: Vec<(&str, Duration, Tone)> = parts(time)
        .into_iter()
        .filter(|(_, spent, _)| spent.as_secs() > 0)
        .collect();
    if parts.is_empty() {
        return Vec::new();
    }
    let room = look.width.cells();
    let seconds: Vec<f64> = parts
        .iter()
        .map(|(_, spent, _)| spent.as_secs_f64())
        .collect();
    let whole = look
        .glyphs
        .fine_steps()
        .last()
        .map_or_else(String::new, char::to_string);
    let mut bar = Line::new();
    for ((_, _, tone), cells) in parts.iter().zip(apportioned(&seconds, room)) {
        bar = bar.push(*tone, whole.repeat(cells));
    }
    let mark = look.glyphs.mark(Mark::Running).to_string();
    let mut words = Line::new();
    for (at, (name, spent, tone)) in parts.iter().enumerate() {
        if at > 0 {
            words = words.push(Tone::Faint, format!(" {} ", look.glyphs.sep()));
        }
        words = words
            .push(*tone, mark.as_str())
            .push(Tone::Muted, format!(" {name} {}", duration(*spent)));
    }
    let heading = Line::new().push(Tone::Muted, "where its time went");
    let mut lines = vec![Line::new(), heading, bar];
    lines.extend(words.wrap(room));
    lines
}

/// What of the checks of `time` a red criterion had already decided, said under where its
/// time went on the printed page: none where no check was decided so.
pub(super) fn decided(time: &TimeSpent) -> Option<Line> {
    (time.decided_checks.as_secs() > 0).then(|| {
        Line::new().push(
            Tone::Muted,
            format!(
                "{} of the checks were guards a red criterion had already decided",
                duration(time.decided_checks)
            ),
        )
    })
}

/// Where most of a run's time went, as the sentence under its headline: the
/// share of the run the largest part took, and what it was spent on; a
/// person's part says at which node when `at` names one. Its duration is
/// said under the time's bar, and a node's rounds on its row. Nothing for a
/// run that spent no time.
pub(super) fn gist(time: &TimeSpent, at: Option<&NodeId>) -> Option<String> {
    let total = time.total();
    let people = match at {
        Some(node) => format!("{WAITING_ON_YOU} at {node}"),
        None => WAITING_ON_YOU.to_string(),
    };
    let (spent, on) = [
        (time.working, format!("agents and the engine {AT_WORK}")),
        (time.rails, "formatting what sessions wrote".to_string()),
        (time.checks, "checks on the tasks' trees".to_string()),
        (
            time.measuring,
            "measuring the suite while nothing else ran".to_string(),
        ),
        (time.people, people),
        (
            time.offline,
            "sessions waiting for their service".to_string(),
        ),
        (time.parked, "parked until a resume".to_string()),
        (time.between, "the engine between steps".to_string()),
    ]
    .into_iter()
    .max_by_key(|(spent, _)| *spent)?;
    if spent.as_secs() == 0 || total.is_zero() {
        return None;
    }
    let share = spent.as_secs_f64() / total.as_secs_f64() * 100.0;
    Some(format!("{share:.0}% of it was {on}"))
}

/// Each part of a run's time in the one order, with what it is called, what
/// it took, and the tone it is drawn in.
fn parts(time: &TimeSpent) -> [(&'static str, Duration, Tone); 8] {
    [
        ("working", time.working, Tone::Running),
        ("rails", time.rails, Tone::Running),
        ("checks", time.checks, Tone::Done),
        ("measuring", time.measuring, Tone::Done),
        (WAITING_ON_YOU, time.people, Tone::NeedsYou),
        ("offline", time.offline, Tone::Caution),
        ("parked", time.parked, Tone::Muted),
        ("between", time.between, Tone::Faint),
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
            surco::figures::latency(median),
            look.glyphs.sep(),
            surco::figures::latency(high),
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

    /// Under its heading, a bar of the parts across the line's whole width,
    /// each as long as its share, then each part with its mark, in words
    /// with what it took, in one order; a part under a second is no part,
    /// and what of the checks was decided already is said under them where
    /// the printed page says it.
    #[test]
    fn the_time_a_run_spent_reads_as_a_bar_of_its_parts() {
        let time = TimeSpent {
            working: Duration::from_secs(600),
            checks: Duration::from_secs(300),
            decided_checks: Duration::from_secs(240),
            measuring: Duration::from_secs(120),
            people: Duration::from_millis(400),
            ..Default::default()
        };
        let mut lines = render(&time, &Look::plain());
        lines.extend(decided(&time));
        let drawn = text(&lines);
        let rows: Vec<&str> = drawn
            .lines()
            .skip_while(|line| *line != "where its time went")
            .skip(1)
            .collect();
        let bar = "#".repeat(80);
        assert_eq!(
            rows,
            [
                bar.as_str(),
                "> working 10m00s ; > checks 5m00s ; > measuring 2m00s",
                "4m00s of the checks were guards a red criterion had already decided",
            ]
        );
    }

    /// A decided time under a second is no part of the checks' line.
    #[test]
    fn a_decided_time_under_a_second_is_not_said() {
        let time = TimeSpent {
            checks: Duration::from_secs(300),
            decided_checks: Duration::from_millis(400),
            ..Default::default()
        };
        assert!(decided(&time).is_none());
    }

    /// Each part's mark is in its tone, a person's in the person's, and its
    /// words muted, apart by a faint separator.
    #[test]
    fn each_part_s_mark_takes_its_tone() {
        let time = TimeSpent {
            working: Duration::from_secs(60),
            people: Duration::from_secs(120),
            ..Default::default()
        };
        let lines = render(&time, &Look::plain());
        let toned: Vec<(&str, Tone)> = lines[3]
            .spans()
            .iter()
            .map(|span| (span.text.as_str(), span.tone))
            .collect();
        assert_eq!(
            toned,
            [
                (">", Tone::Running),
                (" working 1m00s", Tone::Muted),
                (" ; ", Tone::Faint),
                (">", Tone::NeedsYou),
                (" waiting on you 2m00s", Tone::Muted),
            ]
        );
    }

    /// The gist says the share of the run its largest part took, and what
    /// it was spent on; a person's part says at which node when one holds
    /// it. Its duration and its rounds are said elsewhere on the page.
    #[test]
    fn the_gist_says_where_a_person_s_time_went() {
        let time = TimeSpent {
            working: Duration::from_secs(600),
            people: Duration::from_secs(5400),
            ..Default::default()
        };
        let node = NodeId::from("approve-plan");
        assert_eq!(
            gist(&time, Some(&node)).as_deref(),
            Some("90% of it was waiting on you at approve-plan")
        );
        assert_eq!(
            gist(&time, None).as_deref(),
            Some("90% of it was waiting on you")
        );
        let working = TimeSpent {
            working: Duration::from_secs(600),
            ..Default::default()
        };
        assert_eq!(
            gist(&working, None).as_deref(),
            Some("100% of it was agents and the engine at work")
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
            "\nrails:\n  before-write p50 3 ms ; p95 3 ms\n"
        );
    }

    #[test]
    fn a_run_that_asked_the_rail_nothing_draws_no_block() {
        assert!(rail_latency(&BTreeMap::new(), &Look::plain()).is_empty());
    }
}
