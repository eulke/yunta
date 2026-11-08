use yunta_core::NodeId;
use yunta_replay::frame::RunFrame;
use yunta_replay::frame::RunPhase;

use super::*;
use surco::motion::Motion;
use surco::paint::Paint;

/// The look of a terminal `cells` wide.
fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: surco::look::Width::row(cells, None),
    }
}

/// The look of an ASCII terminal wide enough for a whole command.
fn ascii() -> Look {
    Look {
        glyphs: Glyphs::Ascii,
        ..look(120)
    }
}

fn drawn(rows: Vec<Line>) -> Vec<String> {
    rows.iter().map(|row| Paint::Plain.line(row)).collect()
}

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

/// A `parallel` group's children sit one step under the group,
/// exactly where a child run sits under the node that bore it.
#[test]
fn the_live_view_indents_a_groups_children_under_it() {
    let frame = RunFrame::sample(&RUN);
    let group = yunta_replay::frame::NodeFrame::sample(&NodeId::from("review"), NodeStanding::ToGo);
    let mut child =
        yunta_replay::frame::NodeFrame::sample(&NodeId::from("review-a"), NodeStanding::ToGo);
    child.group = Some(group.id.clone());

    let top = drawn(node_rows(&frame, &group, None, &look(120), Motion::Moves));
    let under = drawn(node_rows(&frame, &child, None, &look(120), Motion::Moves));
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
    let frame = RunFrame::sample(&RUN);
    let mut node =
        yunta_replay::frame::NodeFrame::sample(&NodeId::from("implement"), NodeStanding::ToGo);
    node.running_tasks = vec![
        yunta_replay::frame::RunningTask {
            id: "T001".into(),
            in_session: false,
        },
        yunta_replay::frame::RunningTask {
            id: "T002".into(),
            in_session: true,
        },
    ];

    let rows = drawn(node_rows(&frame, &node, None, &look(120), Motion::Moves));
    assert!(
        rows.iter()
            .any(|row| row.trim() == "tasks running: T001 (checking criteria), T002"),
        "{rows:?}"
    );
}

/// A node at work, `events` events into its attempt, `age` seconds past the last.
fn at_work(events: u64, age: u64) -> NodeFrame {
    let mut node =
        yunta_replay::frame::NodeFrame::sample(&NodeId::from("implement"), NodeStanding::ToGo);
    node.standing = Standing::Running;
    node.elapsed = Some(Duration::from_secs(40));
    node.last_event_age = Some(Duration::from_secs(age));
    node.events = events;
    node
}

/// The first row of `node`, as a reader sees it.
fn first_row(node: &NodeFrame, motion: Motion) -> String {
    let frame = RunFrame::sample(&RUN);
    drawn(node_rows(&frame, node, None, &look(120), motion)).remove(0)
}

#[test]
fn a_running_node_turns_a_frame_per_event_and_says_how_long_it_has_run() {
    let row = |events| first_row(&at_work(events, 1), Motion::Moves);
    assert_eq!(row(3), "⠸ implement running for 40s");
    assert_eq!(row(4), "⠼ implement running for 40s");
    assert_eq!(row(13), row(3), "ten frames make a turn");
}

#[test]
fn a_node_that_falls_quiet_says_for_how_long_and_its_spin_goes_out() {
    let row = |age| first_row(&at_work(3, age), Motion::Moves);
    assert_eq!(row(9), "⠸ implement running for 40s");
    assert_eq!(row(10), "⠸ implement running for 40s · quiet 10s");
    assert_eq!(row(11), "⠸ implement running for 40s · quiet 11s");

    let frame = RunFrame::sample(&RUN);
    let tone = |age| {
        let rows = node_rows(&frame, &at_work(3, age), None, &look(120), Motion::Moves);
        rows.first()
            .and_then(|row| row.spans().first())
            .map(|span| span.tone)
    };
    assert_eq!(tone(9), Some(Tone::Running));
    assert_eq!(tone(10), Some(Tone::Muted));
}

#[test]
fn a_still_view_keeps_the_mark_and_every_word() {
    let row = |age| first_row(&at_work(3, age), Motion::Still);
    assert_eq!(row(1), "● implement running for 40s");
    assert_eq!(row(11), "● implement running for 40s · quiet 11s");
}

/// A node that waits says the time it worked, bare: it is not how long it has
/// waited, and silence is what waiting is.
#[test]
fn only_a_node_that_runs_turns_or_is_called_quiet() {
    let mut node = at_work(3, 180);
    node.standing = Standing::Waiting;
    assert_eq!(first_row(&node, Motion::Moves), "● implement waiting · 40s");
}

/// The runner a node resolved through stands after what the row says of its time.
#[test]
fn a_row_ends_on_what_it_runs_on() {
    let mut node = at_work(3, 11);
    node.runner = Some(yunta_core::events::ResolvedRunner {
        runner: "planner".into(),
        chosen: yunta_core::RunnerCandidate {
            adapter: "mock".into(),
            model: "mock-model".into(),
            agent: None,
            effort: None,
        },
        discarded: Vec::new(),
    });
    assert_eq!(
        first_row(&node, Motion::Moves),
        "⠸ implement running for 40s · quiet 11s · planner mock/mock-model"
    );
}

/// The calls of a node, newest first as the frame holds them: a run of a
/// program, an edit, and four reads, of two files and of one it cannot show.
fn activity() -> Vec<yunta_core::events::ToolCall> {
    use yunta_core::events::{ToolAction, ToolTarget};
    let call = |action, target: Option<ToolTarget>| yunta_core::events::ToolCall {
        tool_name: Some("any".to_string()),
        action,
        target,
        at: chrono::DateTime::UNIX_EPOCH,
    };
    let path = |path: &str| Some(ToolTarget::of_path(std::path::Path::new(path)));
    vec![
        call(ToolAction::Run, Some(ToolTarget::named("cargo test"))),
        call(ToolAction::Edit, path("crates/cli/src/pack.rs")),
        call(ToolAction::Read, path("b.rs")),
        call(ToolAction::Read, path("a.rs")),
        call(ToolAction::Read, path("a.rs")),
        call(ToolAction::Read, None),
    ]
}

#[test]
fn a_node_row_says_what_its_calls_came_to() {
    let frame = RunFrame::sample(&RUN);
    let mut node =
        yunta_replay::frame::NodeFrame::sample(&NodeId::from("implement"), NodeStanding::ToGo);
    node.activity = activity();

    let rows = drawn(node_rows(&frame, &node, None, &look(120), Motion::Moves));
    assert!(
        rows.iter()
            .any(|row| row.trim() == "read ×4 · edit crates/cli/src/pack.rs · run cargo test"),
        "{rows:?}"
    );
}

#[test]
fn a_row_with_no_room_lets_its_oldest_calls_go() {
    let frame = RunFrame::sample(&RUN);
    let mut node =
        yunta_replay::frame::NodeFrame::sample(&NodeId::from("implement"), NodeStanding::ToGo);
    node.activity = activity();

    let rows = drawn(node_rows(&frame, &node, None, &look(50), Motion::Moves));
    assert!(
        rows.iter()
            .any(|row| row.trim() == "edit crates/cli/src/pack.rs · run cargo test"),
        "{rows:?}"
    );
    let rows = drawn(node_rows(&frame, &node, None, &look(20), Motion::Moves));
    assert!(
        rows.iter().any(|row| row.trim() == "run cargo test"),
        "what it does now is what stays: {rows:?}"
    );
}

/// A run in `phase`, with `prior` as what its workflow's history
/// says.
fn standing(phase: RunPhase, prior: Option<yunta_replay::history::PriorEstimation>) -> RunFrame {
    RunFrame {
        standing: yunta_replay::frame::run_standing(
            &phase,
            &[],
            0,
            &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
        ),
        phase,
        prior,
        elapsed: Some(Duration::from_secs(3)),
        ..RunFrame::sample(&RUN)
    }
}

fn parked() -> RunPhase {
    RunPhase::Waiting {
        on: yunta_replay::frame::WaitingOn::Node {
            node: NodeId::from_static("plan"),
            on: yunta_core::events::NodeWait::Gate { external_ref: None },
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
    assert!(attention(&standing(RunPhase::Running, None), &RUN, false, &ascii()).is_none());
    let row = attention(&standing(parked(), None), &RUN, true, &ascii())
        .map(|row| Paint::Plain.line(&row))
        .unwrap_or_default();
    assert!(
        row.contains(&format!("needs you: yunta resolve-gate {} (", RUN.handle())),
        "it carries the command that answers it, whole: {row}"
    );
    assert!(row.contains("node `plan`"), "and what it is about: {row}");
}

/// A run whose engine asks at its own terminal says whom it asks after the
/// mark and `needs you`, and nothing to type here: the answer is given there.
#[test]
fn the_row_of_a_run_asked_at_its_terminal_says_whom_it_asks() {
    let asking = yunta_replay::registry::Asking {
        pid: yunta_core::Pid::current(),
        prompt: yunta_replay::registry::Prompt {
            node: Some(NodeId::from_static("implement")),
            since: chrono::DateTime::UNIX_EPOCH,
        },
        waited: std::time::Duration::ZERO,
    };

    let row = asked(&asking, &ascii()).map(|row| Paint::Plain.line(&row));

    assert_eq!(
        row.as_deref(),
        Some("? needs you: node `implement` asks at its terminal")
    );
}

/// A pause a person chose draws no row, and the footer says nothing needs them.
#[test]
fn a_run_a_person_paused_draws_no_attention_row() {
    let paused = |reason: yunta_core::events::PauseReason| {
        standing(
            RunPhase::Waiting {
                on: yunta_replay::frame::WaitingOn::Run { reason },
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
        assert!(attention(frame, &RUN, true, &ascii()).is_none());
        let line = Paint::Plain.line(&footer(frame, &Look::plain()));
        assert!(line.ends_with(" nothing needs you"), "{line}");
    }
}

#[test]
fn a_run_waiting_on_a_reset_draws_no_attention_row_and_says_it_waits() {
    let reset = RunPhase::Waiting {
        on: yunta_replay::frame::WaitingOn::Run {
            reason: yunta_core::events::PauseReason::AwaitingReset {
                node: NodeId::from_static("build"),
                at: yunta_testkit_core::fixed_now(),
            },
        },
    };
    let frame = standing(reset, None);
    assert!(attention(&frame, &RUN, true, &ascii()).is_none());
    let line = Paint::Plain.line(&footer(&frame, &Look::plain()));
    assert!(line.ends_with(" waiting for a reset"), "{line}");
    assert!(!line.contains("needs you"), "{line}");
}

#[test]
fn the_estimate_appears_beside_the_time_once_the_workflow_has_a_history() {
    let paint = |frame: &RunFrame| Paint::Plain.line(&footer(frame, &Look::plain()));
    let known = yunta_replay::history::PriorEstimation {
        sample_count: 3,
        tokens: yunta_replay::history::Percentiles {
            median: 1_000.0,
            p90: 2_000.0,
        },
        wall_clock_secs: Some(yunta_replay::history::Percentiles {
            median: 6_840.0,
            p90: 9_000.0,
        }),
        tasks: yunta_replay::history::Percentiles {
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
    let ahead = |id: &'static str| {
        yunta_replay::frame::NodeFrame::sample(&NodeId::from_static(id), NodeStanding::ToGo)
    };
    let frame = RunFrame {
        nodes: vec![ahead("plan"), ahead("build"), ahead("ship"), ahead("tests")],
        flow: yunta_replay::frame::Counter {
            total: 4,
            ..yunta_replay::frame::Counter::default()
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
    let ahead = |id: &'static str| {
        yunta_replay::frame::NodeFrame::sample(&NodeId::from_static(id), NodeStanding::ToGo)
    };
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
