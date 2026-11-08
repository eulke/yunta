use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use surco::golden::{assert_golden, ENVIRONMENTS};
use yunta_core::events::{
    EventPayload, NodeEvent, NodeFinishedPayload, NodeOutcome, NodeStartedPayload, Spend,
    TokenUsage,
};
use yunta_replay::frame::{Declares, GateAsks, NodeFrame};

use super::*;
use crate::output::surface_lines;

const RUN: yunta_core::RunId = yunta_core::RunId::from_static("01ARZ3NDEKTSV4RRFFQ69G5FAV");

/// The usage the golden page reads, with the cache figure it reports: the
/// sum of what its two nodes spent.
const PAGE_USAGE: TokenUsage = TokenUsage {
    input: 18_000,
    output: 1_800,
    cached: Some(25_200),
};

/// A spend whose usage has all settled: nothing is in flight.
fn settled(usage: TokenUsage) -> Spend {
    Spend {
        settled: usage,
        in_flight: TokenUsage::default(),
    }
}

/// A run with one task done, its documents and findings counted, and
/// `nodes` under it.
fn stats(nodes: Vec<NodeStat>) -> RunStats {
    let spend = settled(TokenUsage {
        input: 300,
        output: 200,
        cached: None,
    });
    RunStats {
        cptv: Some(500.0),
        rework_rate: None,
        cache_rate: None,
        total_tokens: spend.so_far(),
        spend,
        tasks_total: 2,
        tasks_done: 1,
        wall_clock: Some(Duration::from_secs(90)),
        asleep: Duration::ZERO,
        time: Default::default(),
        people_by_node: BTreeMap::new(),
        rails: BTreeMap::new(),
        nodes,
        unknown_kinds: Vec::new(),
        artifact_submissions: yunta_replay::Submissions {
            accepted: 3,
            refused: 1,
        },
        submissions_by_node: Default::default(),
        findings: yunta_replay::FindingActivity {
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

/// The block of one run, drawn for `look`.
fn render_run_stats(
    stats: &RunStats,
    nodes: &[NodeFrame],
    pricing: Option<&BTreeMap<String, yunta_core::PricingEntry>>,
    look: &Look,
) -> String {
    // The header reads the run's own time, as its stats measured it.
    let frame = RunFrame {
        nodes: nodes.to_vec(),
        elapsed: stats.wall_clock,
        ..yunta_replay::frame::RunFrame::sample(&RUN)
    };
    let page = RunStatsPage {
        frame: &frame,
        stats,
        pricing,
        depth: Depth::Glance,
    };
    crate::output::draw(page.document(look), look)
}

fn render(stats: &RunStats, look: &Look) -> String {
    render_run_stats(stats, &[], None, look)
}

#[test]
fn stats_renders_to_a_string_a_test_can_read() {
    // What this command says about a run is one block, not a run of
    // `println!`s: a number nobody can read back is a number nobody
    // is holding to anything. The two counts a run's documents and
    // findings come to are in it, each said as itself.
    let look = Look::plain();
    let run_stats = stats(Vec::new());
    let frame = yunta_replay::frame::RunFrame::sample(&RUN);
    let page = RunStatsPage {
        frame: &frame,
        stats: &run_stats,
        pricing: None,
        depth: Depth::Glance,
    };
    let rows: Vec<String> = surface_lines(page.document(&look), &look)
        .iter()
        .map(Line::text)
        .collect();
    for row in [
        "  task cost    500 tokens per verified task ; 1 task done",
        "  documents    3 accepted, 1 refused",
        "  findings     5 posted, 1 updated, 1 withdrawn; 4 standing",
        "  answers      2 given, 1 proved; 1 settled",
    ] {
        assert!(rows.iter().any(|line| line == row), "`{row}` in: {rows:?}");
    }
}

#[test]
fn a_fact_with_nothing_to_say_is_not_a_row() {
    let quiet = RunStats {
        total_tokens: Default::default(),
        spend: Spend::default(),
        artifact_submissions: Default::default(),
        findings: Default::default(),
        findings_effective: 0,
        findings_settled: 0,
        rails: BTreeMap::new(),
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
        rails: BTreeMap::new(),
        ..stats(Vec::new())
    };
    let text = render(&cached, &Look::plain());
    assert!(
        text.lines()
            .any(|line| line == "  cache        1.4x cached tokens per input token"),
        "{text}"
    );
}

/// One node that ran to its finish: `input` tokens in, a tenth as many
/// out, and `secs` seconds of work, a quarter of it spent blocked.
fn node_stat(id: &str, input: u64, secs: u64) -> NodeStat {
    NodeStat {
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
        models: Vec::new(),
    }
}

/// The run the golden page is drawn from: one task done, its two nodes
/// run, and the cost, cache rate and rework the page reads.
fn golden_run() -> RunStats {
    let spend = settled(PAGE_USAGE);
    // The cost per verified task is the page's input and output over its
    // one task done; the run took 7m30s, longer than the 7m5s its two
    // nodes took between them.
    RunStats {
        cptv: Some(19_800.0),
        rework_rate: Some(0.12),
        cache_rate: Some(1.4),
        total_tokens: spend.so_far(),
        spend,
        rails: BTreeMap::new(),
        wall_clock: Some(Duration::from_secs(450)),
        ..stats(vec![
            node_stat("plan", 4_000, 40),
            node_stat("implement-the-whole-feature", 14_000, 300),
        ])
    }
}

/// The frame of each node of `run`, every one run to its finish in a log
/// the engine derives its state from.
fn finished_frames(run: &RunStats) -> Vec<NodeFrame> {
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
                    NodeOutcome::Summary("exit 0".to_string()),
                    TokenUsage::default(),
                ))),
            );
    }
    let state = yunta_replay::RunState::replay(&log.build());
    // Each node as a frame stands it: finished, the word its row opens with.
    run.nodes
        .iter()
        .map(|stat| {
            yunta_replay::frame::NodeFrame::sample(
                &stat.node_id,
                yunta_replay::frame::NodeStanding::Reached(
                    state
                        .nodes
                        .state(&stat.node_id)
                        .cloned()
                        .expect("every node the log ran has a state"),
                ),
            )
        })
        .collect()
}

#[test]
fn a_run_page_matches_its_goldens() {
    let run = golden_run();
    let nodes = finished_frames(&run);
    let goldens: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/stats");
    for environment in &ENVIRONMENTS {
        assert_golden(
            &environment.golden(&goldens, "run"),
            &render_run_stats(&run, &nodes, None, &surco::look::Look::of(environment)),
        );
    }
}

/// The look of a terminal that draws Unicode glyphs, `width` cells wide.
fn unicode(width: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        width: surco::look::Width::of(None, Some(width)),
        ..Look::plain()
    }
}

/// A node's frame: a plain node, or, when `people` is not zero, a gate a
/// person was asked about for `people`, decided `rounds` times.
fn frame(id: &str, people: Duration, rounds: usize) -> NodeFrame {
    let gate = (!people.is_zero()).then_some(GateAsks {
        question: None,
        on_forge: false,
    });
    NodeFrame {
        kind: match gate {
            Some(_) => "gate",
            None => "bash",
        },
        people,
        decisions: rounds,
        declares: Declares {
            gate,
            on_failure: None,
        },
        ..yunta_replay::frame::NodeFrame::sample(
            &yunta_core::NodeId::from(id),
            yunta_replay::frame::NodeStanding::ToGo,
        )
    }
}

/// The lines of `run` at `depth`, each as the text a reader sees.
fn lines_at(run: &RunStats, nodes: &[NodeFrame], depth: Depth, look: &Look) -> Vec<String> {
    let view = RunFrame {
        nodes: nodes.to_vec(),
        elapsed: run.wall_clock,
        ..yunta_replay::frame::RunFrame::sample(&RUN)
    };
    let page = RunStatsPage {
        frame: &view,
        stats: run,
        pricing: None,
        depth,
    };
    surface_lines(page.document(look), look)
        .iter()
        .map(Line::text)
        .collect()
}

/// The rows under `title`: the lines after it, up to the blank line that
/// ends its block.
fn rows_under<'a>(lines: &'a [String], title: &str) -> Vec<&'a str> {
    lines
        .iter()
        .skip_while(|line| line.as_str() != title)
        .skip(1)
        .take_while(|line| !line.is_empty())
        .map(String::as_str)
        .collect()
}

/// Three nodes that show each kind of row. `plan` and `implement` did their
/// work, `implement` in the given number of attempts, and `approve-plan` is a
/// gate a person was asked about for 6h27m over three rounds. `plan` runs on
/// the `reviewer` runner and carries the model and the blocked share that the
/// whole story adds.
fn asked_run(attempts: u32) -> (RunStats, Vec<NodeFrame>) {
    let plan = NodeStat {
        runner: Some(yunta_core::RunnerName::from("reviewer")),
        models: vec![yunta_core::ModelName::from("haiku-5")],
        ..node_stat("plan", 4_000, 91)
    };
    let implement = NodeStat {
        attempts,
        ..node_stat("implement-the-whole-feature", 14_000, 1_465)
    };
    let approve = NodeStat {
        runner: Some(yunta_core::RunnerName::from("reviewer")),
        ..node_stat("approve-plan", 0, 2)
    };
    let mut before_write = yunta_core::rail::timing::Histogram::default();
    before_write.observe(Duration::from_millis(3));
    let run = RunStats {
        people_by_node: BTreeMap::from([(
            yunta_core::NodeId::from("approve-plan"),
            Duration::from_secs(23_220),
        )]),
        rails: BTreeMap::from([(yunta_core::rail::RailEvent::BeforeWrite, before_write)]),
        ..stats(vec![plan, implement, approve])
    };
    let frames = vec![
        frame("plan", Duration::ZERO, 0),
        frame("implement-the-whole-feature", Duration::ZERO, 0),
        frame("approve-plan", Duration::from_secs(23_220), 3),
    ];
    (run, frames)
}

/// A bar measures its work against the longest work no person was asked
/// about: 1m31s is a cell and a quarter of 24m25s over twenty cells, and a
/// person's wait past that scale is the whole track, cut over its middle.
#[test]
fn a_bar_measures_its_work_against_the_longest_and_a_wait_past_it_is_cut() {
    let (run, frames) = asked_run(2);
    let lines = lines_at(&run, &frames, Depth::Glance, &unicode(120));
    let rows = rows_under(&lines, "nodes:");
    assert_eq!(rows.len(), 3, "{lines:?}");
    assert!(
        rows[0].contains(&format!(" █▎{}", " ".repeat(18))),
        "{}",
        rows[0]
    );
    assert!(
        rows[1].contains(&format!(" {}", "█".repeat(20))),
        "{}",
        rows[1]
    );
    let cut = format!(" {}╱╱{}", "░".repeat(9), "░".repeat(9));
    assert!(rows[2].contains(&cut), "{}", rows[2]);
    assert!(rows[2].contains("6h27m"), "{}", rows[2]);
    assert!(rows[2].ends_with("waiting on you, 3 rounds"), "{}", rows[2]);
}

/// A person's wait names its rounds only when there was more than one.
#[test]
fn a_persons_wait_names_its_rounds_only_past_one() {
    assert_eq!(waiting_on_you(0), "waiting on you");
    assert_eq!(waiting_on_you(1), "waiting on you");
    assert_eq!(waiting_on_you(3), "waiting on you, 3 rounds");
}

/// Every line under the nodes fits the line it is drawn on, at a glance and
/// in the whole story, at each width a line has, with a long id and a
/// person's rounds on the page, whether or not a node took a second attempt:
/// the bar goes first, then the id gives way.
#[test]
fn every_node_row_fits_the_line_it_is_drawn_on() {
    for attempts in [1, 2] {
        let (run, frames) = asked_run(attempts);
        for depth in [Depth::Glance, Depth::Full] {
            for width in [60_usize, 80, 120] {
                let lines = lines_at(&run, &frames, depth, &unicode(width));
                for row in rows_under(&lines, "nodes:") {
                    assert!(cell_width(row) <= width, "{width} cells: `{row}`");
                }
            }
        }
    }
}

/// A long id is whole where its row has room for it beside a bar, and gives
/// way on a narrower line, where the bar goes first and a person's rounds
/// stay said; the whole story by trace never cuts it.
#[test]
fn an_id_is_whole_where_the_row_has_room_for_it() {
    let (run, frames) = asked_run(1);
    let id = "implement-the-whole-feature";
    let at = |depth, width| lines_at(&run, &frames, depth, &unicode(width));
    let wide = at(Depth::Glance, 100);
    let implement = rows_under(&wide, "nodes:")[1];
    assert!(implement.contains(id), "{implement}");
    assert!(implement.contains('█'), "{implement}");
    let middle = at(Depth::Glance, 80);
    let rows = rows_under(&middle, "nodes:");
    assert!(rows[2].ends_with("waiting on you, 3 rounds"), "{}", rows[2]);
    assert!(rows.iter().all(|row| !row.contains('█')), "{rows:?}");
    let narrow = at(Depth::Glance, 60);
    let rows = rows_under(&narrow, "nodes:");
    assert!(!rows[1].contains(id), "{}", rows[1]);
    assert!(
        rows.iter()
            .all(|row| !row.contains('█') && !row.contains('░')),
        "{rows:?}"
    );
    let dig = at(Depth::Dig, 60);
    assert!(dig.iter().any(|line| line.contains(id)), "{dig:?}");
}

/// A node or a runner that spent no token says nothing of tokens.
#[test]
fn a_node_that_spent_nothing_says_no_tokens() {
    let (run, frames) = asked_run(1);
    let lines = lines_at(&run, &frames, Depth::Glance, &unicode(120));
    assert!(
        lines.iter().all(|line| !line.contains(" 0 tokens")),
        "{lines:?}"
    );
}

/// The whole story adds the rails, each node's blocked share and its
/// models; a glance leaves all three out.
#[test]
fn the_whole_story_adds_the_rails_the_blocked_share_and_the_models() {
    let (run, frames) = asked_run(2);
    let look = unicode(120);
    let glance = lines_at(&run, &frames, Depth::Glance, &look).join("\n");
    assert!(!glance.contains("rails:"), "{glance}");
    assert!(!glance.contains("blocked"), "{glance}");
    assert!(!glance.contains("haiku-5"), "{glance}");
    let full = lines_at(&run, &frames, Depth::Full, &look);
    assert!(
        full.iter().any(|line| line.as_str() == "rails:"),
        "{full:?}"
    );
    let rows = rows_under(&full, "nodes:");
    assert!(!rows[0].contains("blocked"), "{}", rows[0]);
    assert_eq!(rows[1].trim(), "19% blocked · haiku-5", "{}", rows[1]);
}

/// Each runner's tokens are a fine bar against the runner that spent the
/// most, and that runner's own bar is whole.
#[test]
fn each_runner_is_a_fine_bar_of_its_tokens_against_the_one_that_spent_most() {
    let (run, frames) = asked_run(2);
    let lines = lines_at(&run, &frames, Depth::Glance, &unicode(120));
    let rows = rows_under(&lines, "runners:");
    assert_eq!(rows.len(), 2, "{lines:?}");
    assert!(
        rows[0].contains(&format!(" █████▊{}", " ".repeat(14))),
        "{}",
        rows[0]
    );
    assert!(
        rows[1].contains(&format!(" {}", "█".repeat(20))),
        "{}",
        rows[1]
    );
}
