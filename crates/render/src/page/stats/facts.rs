//! The facts a run's stats page opens with, each said in one sentence:
//! what a verified task cost, what retries took, what the cache served,
//! what the run spent, handed over, found and settled.

use crate::spend::Breakdown;
use surco::figures::{Ratio, Tokens};
use surco::glyphs::Glyphs;
use yunta_replay::stats::RunStats;

/// What a verified task cost — the rate a run is read by first, and the
/// one its acronym names.
pub(super) fn cptv(stats: &RunStats, glyphs: Glyphs) -> String {
    match stats.cptv {
        Some(cptv) => format!(
            "{} per verified task {} {}",
            Tokens::rounded(cptv),
            glyphs.sep(),
            surco::text::counted_as(stats.tasks_done, "task done", "tasks done")
        ),
        None => "n/a: no task is done, so no cost per verified task".to_string(),
    }
}

/// The share of tokens retries and reroutes took. Nothing for a run with
/// nothing to divide, or none that went to them.
pub(super) fn rework(stats: &RunStats, glyphs: Glyphs) -> String {
    stats
        .rework_rate
        .filter(|rate| *rate > 0.0)
        .map(|rate| {
            format!(
                "{} of tokens went to retries and reroutes",
                Ratio(rate).reads(glyphs.times())
            )
        })
        .unwrap_or_default()
}

/// Cached tokens per input token — past one when a cache serves more
/// than a session sends. Said unknown only for a run that spent tokens
/// its adapter never broke down; nothing for a cache that served none.
pub(super) fn cache(stats: &RunStats, glyphs: Glyphs) -> String {
    match stats.cache_rate {
        Some(rate) if rate > 0.0 => format!(
            "{} cached tokens per input token",
            Ratio(rate).reads(glyphs.times())
        ),
        Some(_) => String::new(),
        None if stats.total_tokens.total() > 0 => "n/a: the adapter never reported it".to_string(),
        None => String::new(),
    }
}

/// What the run spent, in tokens. Nothing for a run that spent none.
pub(super) fn spent(stats: &RunStats) -> String {
    let tokens = &stats.total_tokens;
    match tokens.total() {
        0 => String::new(),
        _ => Breakdown(tokens).to_string(),
    }
}

/// What the run handed over, by what the engine answered — the other
/// half of what a run cost, beside the tokens it spent. A count of zero is
/// no part of it.
pub(super) fn submissions(submissions: &yunta_replay::Submissions) -> String {
    nonzero(&[
        (submissions.accepted, "accepted"),
        (submissions.refused, "refused"),
    ])
}

/// What the run found, by what the engine answered, and how many of
/// those findings stand now.
///
/// The calls and the standing count are two different facts and are
/// said as two: a log carrying five posts, one update and one
/// withdrawal stands at four, and a reader shown only one of those
/// numbers draws the wrong conclusion from either. A count of zero is no
/// part of it.
pub(super) fn findings(activity: &yunta_replay::FindingActivity, effective: u64) -> String {
    let calls = nonzero(&[
        (activity.posted, "posted"),
        (activity.updated, "updated"),
        (activity.withdrawn, "withdrawn"),
        (activity.refused, "refused"),
    ]);
    joined(calls, nonzero(&[(effective, "standing")]))
}

/// The answers given to the run's findings, and how many settled one. A
/// count of zero is no part of it.
pub(super) fn answers(activity: &yunta_replay::FindingActivity, settled: u64) -> String {
    let given = nonzero(&[(activity.answered, "given"), (activity.proved, "proved")]);
    joined(given, nonzero(&[(settled, "settled")]))
}

/// The counts of `parts` that are not zero, each with what it counts, apart
/// by commas: a zero says nothing, so it is no part.
fn nonzero(parts: &[(u64, &str)]) -> String {
    parts
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| format!("{count} {what}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Two facts of one row, apart by a semicolon, either left out when it says
/// nothing.
fn joined(first: String, second: String) -> String {
    match (first.is_empty(), second.is_empty()) {
        (true, _) => second,
        (_, true) => first,
        (false, false) => format!("{first}; {second}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A count of zero is no part of its fact, and a fact of zeros says
    /// nothing.
    #[test]
    fn a_count_of_zero_is_no_part_of_its_fact() {
        let submissions = yunta_replay::Submissions {
            accepted: 3,
            refused: 0,
        };
        assert_eq!(super::submissions(&submissions), "3 accepted");
        let activity = yunta_replay::FindingActivity {
            posted: 5,
            ..Default::default()
        };
        assert_eq!(findings(&activity, 4), "5 posted; 4 standing");
        assert_eq!(answers(&activity, 0), "");
        let answered = yunta_replay::FindingActivity {
            answered: 2,
            ..Default::default()
        };
        assert_eq!(answers(&answered, 2), "2 given; 2 settled");
    }

    /// A count of tasks done reads in the plural past one.
    #[test]
    fn tasks_done_read_in_the_plural() {
        assert_eq!(
            surco::text::counted_as(2, "task done", "tasks done"),
            "2 tasks done"
        );
        assert_eq!(
            surco::text::counted_as(1, "task done", "tasks done"),
            "1 task done"
        );
    }
}
