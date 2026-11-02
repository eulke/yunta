//! Where one run's wall-clock went: the sentence that says where most of it
//! went, and a bar of its parts with each part in words under it.

use std::collections::BTreeMap;
use std::time::Duration;

use surco::bars::parts as apportioned;
use surco::figures::duration;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::width::{cell_width, id_column, middle_cut, INDENT};
use surco::words::WAITING_ON_YOU;
use yunta_core::rail::timing::Histogram;
use yunta_core::rail::RailEvent;
use yunta_core::NodeId;
use yunta_engine::TimeSpent;

/// The block `yunta stats <run>` shows of `time`: a bar of its parts, side
/// by side in the one order two runs are read in, each part as many cells
/// as its share and in its tone, and under it each part in words, in the
/// same tone, with what it took. A part under a second is no part. What of
/// the checks a red criterion had already decided is said under them.
/// Nothing for a run that spent no time at all.
pub(super) fn render(time: &TimeSpent, look: &Look) -> Vec<Line> {
    let parts: Vec<(&str, Duration, Tone)> = parts(time)
        .into_iter()
        .filter(|(_, spent, _)| spent.as_secs() > 0)
        .collect();
    if parts.is_empty() {
        return Vec::new();
    }
    let room = look.width.cells().saturating_sub(cell_width(INDENT));
    let seconds: Vec<f64> = parts
        .iter()
        .map(|(_, spent, _)| spent.as_secs_f64())
        .collect();
    let whole = look
        .glyphs
        .fine_steps()
        .last()
        .map_or_else(String::new, char::to_string);
    let mut bar = Line::new().plain(INDENT);
    for ((_, _, tone), cells) in parts.iter().zip(apportioned(&seconds, room)) {
        bar = bar.push(*tone, whole.repeat(cells));
    }
    let mut words = Line::new();
    for (at, (name, spent, tone)) in parts.iter().enumerate() {
        if at > 0 {
            words = words.push(Tone::Muted, format!(" {} ", look.glyphs.sep()));
        }
        words = words.push(*tone, format!("{name} {}", duration(*spent)));
    }
    let mut lines = vec![Line::new(), Line::new().plain("time:"), bar];
    lines.extend(words.wrap(room).into_iter().map(|line| line.under(INDENT)));
    if time.decided_checks.as_secs() > 0 {
        lines.push(Line::new().plain(INDENT).push(
            Tone::Muted,
            format!(
                "{} of the checks were guards a red criterion had already decided",
                duration(time.decided_checks)
            ),
        ));
    }
    lines
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
        (time.working, "agents and the engine at work".to_string()),
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

    /// A bar of the parts, each as long as its share, then each part in
    /// words with what it took, in one order; a part under a second is no
    /// part, and what of the checks was decided already is said under them.
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
        let drawn = text(&render(&time, &Look::plain()));
        let rows: Vec<&str> = drawn
            .lines()
            .skip_while(|line| *line != "time:")
            .skip(1)
            .collect();
        let bar = format!("  {}", "#".repeat(78));
        assert_eq!(
            rows,
            [
                bar.as_str(),
                "  working 10m00s ; checks 5m00s ; measuring 2m00s",
                "  4m00s of the checks were guards a red criterion had already decided",
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
        let drawn = text(&render(&time, &Look::plain()));
        assert!(!drawn.contains("of the checks were guards"), "{drawn}");
    }

    /// Each part's words are in its tone, a person's in the person's.
    #[test]
    fn each_part_s_words_take_its_tone() {
        let time = TimeSpent {
            working: Duration::from_secs(60),
            people: Duration::from_secs(120),
            ..Default::default()
        };
        let lines = render(&time, &Look::plain());
        let toned: Vec<(&str, Tone)> = lines[3]
            .spans()
            .iter()
            .filter(|span| !matches!(span.tone, Tone::Plain | Tone::Muted))
            .map(|span| (span.text.as_str(), span.tone))
            .collect();
        assert_eq!(
            toned,
            [
                ("working 1m00s", Tone::Running),
                ("waiting on you 2m00s", Tone::NeedsYou),
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
