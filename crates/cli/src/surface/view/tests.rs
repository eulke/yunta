use yunta_core::NodeId;
use yunta_engine::RunPhase;
use yunta_testkit::run_frame;

use super::*;
use surco::paint::Paint;

fn drawn(rows: Vec<Line>) -> Vec<String> {
    rows.iter().map(|row| Paint::Plain.line(row)).collect()
}

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

/// A `parallel` group's children sit one step under the group,
/// exactly where a child run sits under the node that bore it.
#[test]
fn the_live_view_indents_a_groups_children_under_it() {
    let frame = run_frame(&RUN);
    let group = yunta_testkit::node_frame(&NodeId::from("review"), NodeStanding::ToGo);
    let mut child = yunta_testkit::node_frame(&NodeId::from("review-a"), NodeStanding::ToGo);
    child.group = Some(group.id.clone());

    let top = drawn(node_rows(&frame, &group, Glyphs::Ascii));
    let under = drawn(node_rows(&frame, &child, Glyphs::Ascii));
    assert!(
        !top[0].starts_with(' '),
        "a top-level node starts at the margin: {top:?}"
    );
    assert!(
        under[0].starts_with(&indent(CHILD_DEPTH)) && !under[0].starts_with(&indent(2)),
        "its children sit exactly one step under it: {under:?}"
    );
}

#[test]
fn a_running_task_no_session_works_is_said_to_be_checking_its_criteria() {
    let frame = run_frame(&RUN);
    let mut node = yunta_testkit::node_frame(&NodeId::from("implement"), NodeStanding::ToGo);
    node.running_tasks = vec![
        yunta_engine::RunningTask {
            id: "T001".into(),
            in_session: false,
        },
        yunta_engine::RunningTask {
            id: "T002".into(),
            in_session: true,
        },
    ];

    let rows = drawn(node_rows(&frame, &node, Glyphs::Ascii));
    assert!(
        rows.iter()
            .any(|row| row.trim() == "tasks running: T001 (checking criteria), T002"),
        "{rows:?}"
    );
}

#[test]
fn a_node_row_names_what_its_calls_touched() {
    let frame = run_frame(&RUN);
    let mut node = yunta_testkit::node_frame(&NodeId::from("implement"), NodeStanding::ToGo);
    let call =
        |tool: &str, target: Option<yunta_core::events::ToolTarget>| yunta_engine::ToolCall {
            tool_name: Some(tool.to_string()),
            target,
            at: chrono::DateTime::UNIX_EPOCH,
        };
    node.activity = vec![
        call(
            "Edit",
            Some(yunta_core::events::ToolTarget::of_path(
                std::path::Path::new("src/lib.rs"),
            )),
        ),
        call("Bash", None),
    ];

    let rows = drawn(node_rows(&frame, &node, Glyphs::Ascii));
    assert!(
        rows.iter()
            .any(|row| row.trim() == "calls: Edit src/lib.rs, Bash"),
        "{rows:?}"
    );
}

/// A run in `phase`, with `prior` as what its workflow's history
/// says.
fn standing(phase: RunPhase, prior: Option<yunta_engine::PriorEstimation>) -> RunFrame {
    RunFrame {
        standing: yunta_engine::run_standing(
            &phase,
            &[],
            0,
            &yunta_engine::Engine::this(chrono::DateTime::UNIX_EPOCH),
        ),
        phase,
        prior,
        elapsed: Some(Duration::from_secs(3)),
        ..run_frame(&RUN)
    }
}

fn parked() -> RunPhase {
    RunPhase::Waiting {
        on: yunta_engine::WaitingOn::Node {
            node: NodeId::from_static("plan"),
            on: yunta_engine::NodeWait::Gate { external_ref: None },
            reason: None,
        },
    }
}

#[test]
fn the_footer_says_nothing_needs_you_while_the_run_moves() {
    let paint = |frame: &RunFrame| Paint::Plain.line(&footer(frame, &Look::plain()));
    let moving = paint(&standing(RunPhase::Running, None));
    assert!(moving.ends_with(" nothing needs you"), "{moving}");
    assert!(moving.contains("3s"), "how long it has taken: {moving}");
    let waiting = paint(&standing(parked(), None));
    assert!(
        waiting.ends_with(" needs you") && !waiting.contains("nothing"),
        "{waiting}"
    );
}

#[test]
fn the_footer_names_what_the_run_spent_in_and_out() {
    let paint = |frame: &RunFrame| Paint::Plain.line(&footer(frame, &Look::plain()));
    let spent = RunFrame {
        tokens: yunta_core::events::TokenUsage {
            input: 18_000,
            output: 2_200,
            cached: None,
        },
        ..standing(RunPhase::Running, None)
    };
    let line = paint(&spent);
    assert!(line.contains("18k in / 2.2k out"), "{line}");
    let quiet = RunFrame {
        tokens: yunta_core::events::TokenUsage::default(),
        ..standing(RunPhase::Running, None)
    };
    assert!(
        !paint(&quiet).contains(" in / "),
        "a run that spent nothing names no tokens"
    );
}

#[test]
fn the_attention_row_is_drawn_only_while_something_needs_you() {
    assert!(attention(
        &standing(RunPhase::Running, None),
        &RUN,
        false,
        Glyphs::Ascii
    )
    .is_none());
    let row = attention(&standing(parked(), None), &RUN, true, Glyphs::Ascii)
        .map(|row| Paint::Plain.line(&row))
        .unwrap_or_default();
    assert!(
        row.contains(&format!("needs you: yunta resolve-gate {} (", RUN.handle())),
        "it carries the command that answers it, whole: {row}"
    );
    assert!(row.contains("node `plan`"), "and what it is about: {row}");
}

/// A pause a person chose draws no row, and the footer says nothing needs them.
#[test]
fn a_run_a_person_paused_draws_no_attention_row() {
    let paused = |reason: yunta_core::events::PauseReason| {
        standing(
            RunPhase::Waiting {
                on: yunta_engine::WaitingOn::Run { reason },
            },
            None,
        )
    };
    let closed = paused(yunta_core::events::PauseReason::PullRequestClosed {
        node: NodeId::from_static("approve"),
        url: "https://forge/pr/1".to_string(),
    });
    let aborted = paused(yunta_core::events::PauseReason::GateAborted {
        node: NodeId::from_static("approve"),
        free_text: None,
    });
    for frame in [&closed, &aborted] {
        assert!(attention(frame, &RUN, true, Glyphs::Ascii).is_none());
        let line = Paint::Plain.line(&footer(frame, &Look::plain()));
        assert!(line.ends_with(" nothing needs you"), "{line}");
    }
}

#[test]
fn a_run_waiting_on_a_reset_draws_no_attention_row_and_says_it_waits() {
    let reset = RunPhase::Waiting {
        on: yunta_engine::WaitingOn::Run {
            reason: yunta_core::events::PauseReason::AwaitingReset {
                node: NodeId::from_static("build"),
                at: yunta_testkit_core::fixed_now(),
            },
        },
    };
    let frame = standing(reset, None);
    assert!(attention(&frame, &RUN, true, Glyphs::Ascii).is_none());
    let line = Paint::Plain.line(&footer(&frame, &Look::plain()));
    assert!(line.ends_with(" waiting for a reset"), "{line}");
    assert!(!line.contains("needs you"), "{line}");
}

#[test]
fn the_estimate_appears_beside_the_time_once_the_workflow_has_a_history() {
    let paint = |frame: &RunFrame| Paint::Plain.line(&footer(frame, &Look::plain()));
    let known = yunta_engine::PriorEstimation {
        sample_count: 3,
        tokens: yunta_engine::Percentiles {
            median: 1_000.0,
            p90: 2_000.0,
        },
        wall_clock_secs: Some(yunta_engine::Percentiles {
            median: 6_840.0,
            p90: 9_000.0,
        }),
        tasks: yunta_engine::Percentiles {
            median: 0.0,
            p90: 0.0,
        },
    };
    let first = paint(&standing(RunPhase::Running, None));
    assert!(!first.contains("usually"), "{first}");
    let later = paint(&standing(RunPhase::Running, Some(known)));
    let sep = Glyphs::Ascii.sep();
    assert!(
        later.contains(&format!("3s {sep} usually ~1h54m")),
        "{later}"
    );
}

/// A footer opens with the furrow when the row has room for it, and a row
/// too narrow keeps the counter and its last word, and drops the furrow.
#[test]
fn the_footer_opens_with_the_furrow_when_the_row_has_room() {
    let ahead =
        |id: &'static str| yunta_testkit::node_frame(&NodeId::from_static(id), NodeStanding::ToGo);
    let frame = RunFrame {
        nodes: vec![ahead("plan"), ahead("build"), ahead("ship"), ahead("tests")],
        flow: yunta_engine::Counter {
            total: 4,
            ..yunta_engine::Counter::default()
        },
        ..standing(RunPhase::Running, None)
    };
    let wide = Paint::Plain.line(&footer(&frame, &Look::plain()));
    assert!(wide.starts_with(". .. . . "), "{wide}");
    assert!(wide.ends_with(" nothing needs you"), "{wide}");
    assert!(wide.contains("0 of 4"), "{wide}");
    let narrow = Look {
        width: surco::look::Width::of(None, None).within(40),
        ..Look::plain()
    };
    let line = Paint::Plain.line(&footer(&frame, &narrow));
    assert_eq!(line, "0 of 4 nodes done ; nothing needs you");
}

/// A run of three nodes shows its whole furrow, six cells, on a row that
/// has room for it: the floor is for a furrow squeezed, not a small one.
#[test]
fn a_small_run_shows_its_whole_furrow() {
    let ahead =
        |id: &'static str| yunta_testkit::node_frame(&NodeId::from_static(id), NodeStanding::ToGo);
    let frame = RunFrame {
        nodes: vec![ahead("plan"), ahead("build"), ahead("ship")],
        ..standing(RunPhase::Running, None)
    };
    let row = Paint::Plain.line(&footer(&frame, &Look::plain()));
    assert!(!row.starts_with('0'), "{row}");
    assert!(row.ends_with(" nothing needs you"), "{row}");
}

/// Narrower than the counter and the answer, a row drops the counter and
/// keeps the answer whole.
#[test]
fn the_answer_outlasts_the_counter() {
    let frame = standing(RunPhase::Running, None);
    let narrow = Look {
        width: surco::look::Width::of(None, None).within(20),
        ..Look::plain()
    };
    let row = Paint::Plain.line(&footer(&frame, &narrow));
    assert_eq!(row, "nothing needs you");
}
