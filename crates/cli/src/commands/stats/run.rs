//! `yunta stats <run>`: what one run spent, by node and by runner, and
//! what it handed over and found.

use yunta_core::units::DURATION_WIDEST;
use yunta_core::RunId;
use yunta_engine::{NodeStat, RunStats};

use crate::render::blocks::Fields;
use crate::render::doc::Doc;
use crate::render::{
    bar, bar_cells, cell_width, duration, id_column, middle_cut, truncate, Glyphs, Look,
    NodeDisplay, Ratio, Tokens, INDENT, STATE_WIDTH,
};

pub(super) fn currency_line(
    tokens: u64,
    pricing: Option<&std::collections::BTreeMap<String, yunta_core::PricingEntry>>,
) -> Option<String> {
    // A currency estimate needs a *model* to price against; with more
    // than one model priced and no per-node attribution surfaced here,
    // showing one blended-average line beats showing none — never
    // silently picking the first entry a HashMap happens to iterate.
    let pricing = pricing?;
    if pricing.is_empty() {
        return None;
    }
    let avg_per_1k: f64 = pricing
        .values()
        .map(|entry| entry.cost_per_1k_tokens)
        .sum::<f64>()
        / pricing.len() as f64;
    let estimate = (tokens as f64 / 1000.0) * avg_per_1k;
    Some(format!(
        "~{estimate:.2} (avg of {}, never authoritative)",
        yunta_core::text::counted(pricing.len(), "priced model")
    ))
}

/// The whole `yunta stats <run_id>` block, ready to print on a stream
/// with `look`: the run's facts, a row each that has something to say,
/// then its nodes and its runners.
///
/// A block rather than a run of `println!`s: what this command says
/// about a run is one thing a test reads whole, and a number nobody can
/// assert is a number nobody is holding to anything.
pub(super) fn render_run_stats(
    run_id: &RunId,
    mode: &str,
    stats: &RunStats,
    state: &yunta_engine::RunState,
    pricing: Option<&std::collections::BTreeMap<String, yunta_core::PricingEntry>>,
    look: &Look,
) -> String {
    let glyphs = look.glyphs;
    let asleep = match stats.asleep.is_zero() {
        true => String::new(),
        false => format!("{} — left out of every duration", duration(stats.asleep)),
    };
    let fields = Fields::new()
        .push_if("CPTV", cptv(stats, glyphs))
        .push_if("rework", rework(stats, glyphs))
        .push_if("cache", cache(stats, glyphs))
        .push_if("tokens", spent(stats))
        .push_if(
            "cost",
            currency_line(stats.total_tokens.total(), pricing).unwrap_or_default(),
        )
        .push_if("host asleep", asleep)
        .push_if("documents", submissions(&stats.artifact_submissions))
        .push_if(
            "findings",
            findings(&stats.findings, stats.findings_effective),
        )
        .push_if("answers", answers(&stats.findings, stats.findings_settled))
        .push_if(
            "unread",
            crate::commands::unknown_kinds_note(&stats.unknown_kinds, look.glyphs)
                .unwrap_or_default(),
        );
    let mut out = format!("run {run_id} — mode {mode}\n");
    out.push_str(&crate::render::draw(Doc::new().with(fields), look));
    out.push_str(&super::time::render(&stats.time, look));
    out.push_str(&render_nodes(stats, state, look));
    out.push_str(&render_runners(stats, look));
    out
}

/// What a verified task cost — the rate a run is read by first, and the
/// one its acronym names.
fn cptv(stats: &RunStats, glyphs: Glyphs) -> String {
    match stats.cptv {
        Some(cptv) => format!(
            "{} per verified task {} {}",
            Tokens::rounded(cptv),
            glyphs.sep(),
            yunta_core::text::counted(stats.tasks_done, "task done")
        ),
        None => "n/a — no task is done, so no cost per verified task".to_string(),
    }
}

/// The share of tokens retries and re-routes took. Nothing for a run
/// with nothing to divide.
fn rework(stats: &RunStats, glyphs: Glyphs) -> String {
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
fn cache(stats: &RunStats, glyphs: Glyphs) -> String {
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
fn spent(stats: &RunStats) -> String {
    let tokens = &stats.total_tokens;
    if tokens.total() == 0 {
        return String::new();
    }
    let cached = match tokens.cached {
        Some(cached) => format!(" ({} cached)", Tokens(cached).figure()),
        None => String::new(),
    };
    format!(
        "{} in / {} out{cached}",
        Tokens(tokens.input).figure(),
        Tokens(tokens.output).figure()
    )
}

/// What the run handed over, by what the engine answered — the other
/// half of what a run cost, beside the tokens it spent.
fn submissions(submissions: &yunta_engine::Submissions) -> String {
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
fn findings(activity: &yunta_engine::FindingActivity, effective: u64) -> String {
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
fn answers(activity: &yunta_engine::FindingActivity, settled: u64) -> String {
    if activity.answered + activity.proved == 0 && settled == 0 {
        return String::new();
    }
    format!(
        "{} given, {} proved — {settled} settled",
        activity.answered, activity.proved
    )
}

/// One row per node that started, each opening with the state it is in:
/// the same token count reads one way under a node that finished and
/// another under one that failed, so the number never appears without
/// it.
fn render_nodes(stats: &RunStats, state: &yunta_engine::RunState, look: &Look) -> String {
    if stats.nodes.is_empty() {
        return String::new();
    }
    let max_tokens = stats
        .nodes
        .iter()
        .map(|n| n.tokens.total())
        .max()
        .unwrap_or(0);
    let column = id_column(stats.nodes.iter().map(|node| node.node_id.as_str()));
    // Every column but the bar: the margin, the mark and the word, the
    // id, the tokens, the duration and the blocked share, and the gaps.
    let taken = cell_width(INDENT) + STATE_WIDTH + column + Tokens::WIDEST + DURATION_WIDEST + 29;
    let cells = bar_cells(look.width.cells(), taken);
    let mut out = String::from("\nnodes:\n");
    for node in &stats.nodes {
        let display = NodeDisplay::of(state.nodes.state(&node.node_id));
        out.push_str(&format!(
            "{}\n",
            node_line(node, (max_tokens, column, cells), &display, look)
        ));
    }
    out
}

/// The same tokens grouped by the runner that spent them: where the
/// run's cost went, across however many nodes each runner was given.
fn render_runners(stats: &RunStats, look: &Look) -> String {
    let by_runner = stats.tokens_by_runner();
    if by_runner.is_empty() {
        return String::new();
    }
    let max_runner_tokens = by_runner.iter().map(|(_, t)| t.total()).max().unwrap_or(0);
    let column = id_column(by_runner.iter().map(|(runner, _)| runner.as_str()));
    let cells = bar_cells(
        look.width.cells(),
        cell_width(INDENT) + column + Tokens::WIDEST + 10,
    );
    let mut out = String::from("\nrunners:\n");
    for (runner, tokens) in &by_runner {
        let total = tokens.total();
        out.push_str(&format!(
            "{INDENT}{}{}  {}\n",
            middle_cut(runner.as_str(), column, look.glyphs),
            spaced(bar(total, max_runner_tokens, cells, look.glyphs)),
            Tokens(total).column().trim_end(),
        ));
    }
    out
}

/// `bar` with the space that sets it apart, or nothing for a bar the
/// line had no room for.
fn spaced(bar: String) -> String {
    match bar.is_empty() {
        true => bar,
        false => format!(" {bar}"),
    }
}

/// One node's row, its id in a column `column` cells wide, which is as
/// wide as the longest id the table holds, and its bar `cells` wide.
fn node_line(
    node: &NodeStat,
    (max_tokens, column, cells): (u64, usize, usize),
    display: &NodeDisplay,
    look: &Look,
) -> String {
    let glyphs = look.glyphs;
    let total = node.tokens.total();
    let blocked = node
        .blocked_fraction()
        .map(|fraction| Ratio(fraction).reads(glyphs.times()))
        .unwrap_or_else(|| "n/a".to_string());
    format!(
        "{INDENT}{} {} {}{}  {}  {:>DURATION_WIDEST$}  {blocked:>4} blocked",
        look.ink.mark(glyphs, display.word.mark()),
        truncate(display.word.word(), STATE_WIDTH, glyphs),
        middle_cut(node.node_id.as_str(), column, glyphs),
        spaced(bar(total, max_tokens, cells, glyphs)),
        Tokens(total).column(),
        duration(node.wall_clock()),
    )
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use yunta_core::events::{
        EventPayload, NodeEvent, NodeFinishedPayload, NodeStartedPayload, TokenUsage,
    };
    use yunta_testkit::{assert_golden, ENVIRONMENTS};

    use super::*;

    const RUN: RunId = RunId::from_static("01ARZ3NDEKTSV4RRFFQ69G5FAV");

    /// A run with one task done, its documents and findings counted, and
    /// `nodes` under it.
    fn stats(nodes: Vec<NodeStat>) -> RunStats {
        RunStats {
            cptv: Some(500.0),
            rework_rate: None,
            cache_rate: None,
            total_tokens: yunta_core::events::TokenUsage {
                input: 300,
                output: 200,
                cached: None,
            },
            tasks_total: 2,
            tasks_done: 1,
            wall_clock: Some(Duration::from_secs(90)),
            asleep: Duration::ZERO,
            time: Default::default(),
            nodes,
            unknown_kinds: Vec::new(),
            artifact_submissions: yunta_engine::Submissions {
                accepted: 3,
                refused: 1,
            },
            submissions_by_node: Default::default(),
            findings: yunta_engine::FindingActivity {
                posted: 5,
                updated: 1,
                withdrawn: 1,
                refused: 0,
                answered: 2,
                proved: 1,
            },
            findings_by_node: Default::default(),
            findings_effective: 4,
            findings_settled: 1,
        }
    }

    fn render(stats: &RunStats, look: &Look) -> String {
        render_run_stats(
            &RUN,
            "default",
            stats,
            &yunta_engine::derive(&[]),
            None,
            look,
        )
    }

    #[test]
    fn stats_renders_to_a_string_a_test_can_read() {
        // What this command says about a run is one block, not a run of
        // `println!`s: a number nobody can read back is a number nobody
        // is holding to anything. The two counts a run's documents and
        // findings come to are in it, each said as itself.
        let text = render(&stats(Vec::new()), &Look::plain());
        for row in [
            "  CPTV         500 tokens per verified task | 1 task done",
            "  documents    3 accepted, 1 refused",
            "  findings     5 posted, 1 updated, 1 withdrawn, 0 refused — 4 standing",
            "  answers      2 given, 1 proved — 1 settled",
        ] {
            assert!(text.lines().any(|line| line == row), "`{row}` in: {text}");
        }
    }

    #[test]
    fn a_fact_with_nothing_to_say_is_not_a_row() {
        let quiet = RunStats {
            total_tokens: Default::default(),
            artifact_submissions: Default::default(),
            findings: Default::default(),
            findings_effective: 0,
            findings_settled: 0,
            ..stats(Vec::new())
        };
        let text = render(&quiet, &Look::plain());
        for label in [
            "tokens",
            "cache",
            "rework",
            "documents",
            "findings",
            "answers",
        ] {
            assert!(
                !text.contains(&format!("  {label} ")),
                "an empty `{label}` row: {text}"
            );
        }
    }

    #[test]
    fn the_cache_rate_above_one_reads_as_a_multiple() {
        let cached = RunStats {
            cache_rate: Some(1.4),
            ..stats(Vec::new())
        };
        let text = render(&cached, &Look::plain());
        assert!(
            text.lines()
                .any(|line| line == "  cache        1.4x cached tokens per input token"),
            "{text}"
        );
    }

    #[test]
    fn a_run_page_matches_its_goldens() {
        let node = |id: &str, input: u64, secs: u64| NodeStat {
            node_id: yunta_core::NodeId::from(id),
            runner: Some(yunta_core::RunnerName::from("implementer")),
            tokens: yunta_core::events::TokenUsage {
                input,
                output: input / 10,
                cached: None,
            },
            attempts: 1,
            active: Duration::from_secs(secs),
            open_attempt: None,
            blocked: Duration::from_secs(secs / 4),
        };
        let run = RunStats {
            rework_rate: Some(0.12),
            cache_rate: Some(1.4),
            total_tokens: yunta_core::events::TokenUsage {
                input: 18_000,
                output: 2_200,
                cached: Some(25_200),
            },
            ..stats(vec![
                node("plan", 4_000, 40),
                node("implement-the-whole-feature", 14_000, 300),
            ])
        };
        // Both nodes ran and finished, as the log the page reads says.
        let mut log = yunta_testkit_core::Log::for_run(RUN.as_str());
        for id in ["plan", "implement-the-whole-feature"] {
            log = log
                .node(
                    id,
                    EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
                )
                .node(
                    id,
                    EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
                        "exit 0".to_string(),
                        TokenUsage::default(),
                    ))),
                );
        }
        let state = yunta_engine::derive(&log.build());
        let goldens: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/stats");
        for environment in &ENVIRONMENTS {
            assert_golden(
                &environment.golden(&goldens, "run"),
                &render_run_stats(
                    &RUN,
                    "default",
                    &run,
                    &state,
                    None,
                    &crate::render::look_of(environment),
                ),
            );
        }
    }
}
