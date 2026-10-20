//! The facts a run's stats page opens with, each said in one sentence:
//! what a verified task cost, what retries took, what the cache served,
//! what the run spent, handed over, found and settled.

use surco::figures::{Ratio, Tokens};
use surco::glyphs::Glyphs;
use yunta_engine::RunStats;
use yunta_render::spend::Breakdown;

/// What a verified task cost — the rate a run is read by first, and the
/// one its acronym names.
pub(super) fn cptv(stats: &RunStats, glyphs: Glyphs) -> String {
    match stats.cptv {
        Some(cptv) => format!(
            "{} per verified task {} {}",
            Tokens::rounded(cptv),
            glyphs.sep(),
            surco::text::counted(stats.tasks_done, "task done")
        ),
        None => "n/a — no task is done, so no cost per verified task".to_string(),
    }
}

/// The share of tokens retries and re-routes took. Nothing for a run
/// with nothing to divide.
pub(super) fn rework(stats: &RunStats, glyphs: Glyphs) -> String {
    stats
        .rework_rate
        .map(|rate| {
            format!(
                "{} of tokens went to retries and re-routes",
                Ratio(rate).reads(glyphs.times())
            )
        })
        .unwrap_or_default()
}

/// Cached tokens per input token — past one when a cache serves more
/// than a session sends. Said unknown only for a run that spent tokens
/// its adapter never broke down.
pub(super) fn cache(stats: &RunStats, glyphs: Glyphs) -> String {
    match stats.cache_rate {
        Some(rate) => format!(
            "{} cached tokens per input token",
            Ratio(rate).reads(glyphs.times())
        ),
        None if stats.total_tokens.total() > 0 => "n/a — the adapter never reported it".to_string(),
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
/// half of what a run cost, beside the tokens it spent.
pub(super) fn submissions(submissions: &yunta_engine::Submissions) -> String {
    match submissions.accepted + submissions.refused {
        0 => String::new(),
        _ => format!(
            "{} accepted, {} refused",
            submissions.accepted, submissions.refused
        ),
    }
}

/// What the run found, by what the engine answered, and how many of
/// those findings stand now.
///
/// The calls and the standing count are two different facts and are
/// said as two: a log carrying five posts, one update and one
/// withdrawal stands at four, and a reader shown only one of those
/// numbers draws the wrong conclusion from either.
pub(super) fn findings(activity: &yunta_engine::FindingActivity, effective: u64) -> String {
    let calls = activity.posted + activity.updated + activity.withdrawn + activity.refused;
    if calls == 0 && effective == 0 {
        return String::new();
    }
    format!(
        "{} posted, {} updated, {} withdrawn, {} refused — {effective} standing",
        activity.posted, activity.updated, activity.withdrawn, activity.refused,
    )
}

/// The answers given to the run's findings, and how many settled one.
pub(super) fn answers(activity: &yunta_engine::FindingActivity, settled: u64) -> String {
    if activity.answered + activity.proved == 0 && settled == 0 {
        return String::new();
    }
    format!(
        "{} given, {} proved — {settled} settled",
        activity.answered, activity.proved
    )
}
