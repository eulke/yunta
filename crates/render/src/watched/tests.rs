use surco::motion::Motion;
use surco::paint::Paint;
use yunta_core::NodeId;
use yunta_replay::frame::RunFrame;
use yunta_replay::frame::RunPhase;

use super::*;

/// The look of a terminal `cells` wide.
pub(super) fn look(cells: usize) -> Look {
    Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: surco::look::Width::row(cells, None),
        motion: Motion::Moves,
    }
}

/// The look of an ASCII terminal wide enough for a whole command.
pub(super) fn ascii() -> Look {
    Look {
        glyphs: Glyphs::Ascii,
        ..look(120)
    }
}

/// `rows` as a reader sees them, unpainted.
pub(super) fn drawn(rows: Vec<Line>) -> Vec<String> {
    rows.iter().map(|row| Paint::Plain.line(row)).collect()
}

/// A run's id, for a frame made by hand.
pub(super) const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");

/// A `parallel` group's children sit one step under the group,
/// exactly where a child run sits under the node that bore it.
#[test]
fn the_live_view_indents_a_groups_children_under_it() {
    let frame = RunFrame::sample(&RUN);
    let group = yunta_replay::frame::NodeFrame::sample(&NodeId::from("review"), NodeStanding::ToGo);
    let mut child =
        yunta_replay::frame::NodeFrame::sample(&NodeId::from("review-a"), NodeStanding::ToGo);
    child.group = Some(group.id.clone());

    let top = drawn(node_rows(&frame, &group, None, &look(120)));
    let under = drawn(node_rows(&frame, &child, None, &look(120)));
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

    let rows = drawn(node_rows(&frame, &node, None, &look(120)));
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
    let look = Look {
        motion,
        ..look(120)
    };
    drawn(node_rows(&frame, node, None, &look)).remove(0)
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
        let rows = node_rows(&frame, &at_work(3, age), None, &look(120));
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
        session: None,
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

    let rows = drawn(node_rows(&frame, &node, None, &look(120)));
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

    let rows = drawn(node_rows(&frame, &node, None, &look(50)));
    assert!(
        rows.iter()
            .any(|row| row.trim() == "edit crates/cli/src/pack.rs · run cargo test"),
        "{rows:?}"
    );
    let rows = drawn(node_rows(&frame, &node, None, &look(20)));
    assert!(
        rows.iter().any(|row| row.trim() == "run cargo test"),
        "what it does now is what stays: {rows:?}"
    );
}

/// The ribbon names each node over its stretch, and plows the running node's stretch a
/// cell for each event it has had only where the view moves.
#[test]
fn the_ribbon_plows_the_running_node_only_where_the_view_moves() {
    let frame = RunFrame {
        nodes: vec![at_work(7, 0)],
        ..RunFrame::sample(&RUN)
    };
    let moving = ribbon(&frame, Motion::Moves);
    let names: Vec<&str> = moving
        .stretches
        .iter()
        .map(|stretch| stretch.name.as_str())
        .collect();
    assert_eq!(names, ["implement"]);
    assert_eq!(moving.events, Some(7));
    assert_eq!(ribbon(&frame, Motion::Still).events, None);
}

/// A run in `phase`.
fn standing(phase: RunPhase) -> RunFrame {
    RunFrame {
        standing: yunta_replay::frame::run_standing(
            &phase,
            &[],
            0,
            &yunta_replay::registry::Engine::this(yunta_core::Pid::current()),
        ),
        phase,
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
fn the_attention_row_is_drawn_only_while_something_needs_you() {
    assert!(attention(&standing(RunPhase::Running), &RUN, false, &ascii()).is_none());
    let row = attention(&standing(parked()), &RUN, true, &ascii())
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

/// A pause a person chose draws no row.
#[test]
fn a_run_a_person_paused_draws_no_attention_row() {
    let paused = |reason: yunta_core::events::PauseReason| {
        standing(RunPhase::Waiting {
            on: yunta_replay::frame::WaitingOn::Run { reason },
        })
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
    }
}

#[test]
fn a_run_waiting_on_a_reset_draws_no_attention_row() {
    let reset = RunPhase::Waiting {
        on: yunta_replay::frame::WaitingOn::Run {
            reason: yunta_core::events::PauseReason::AwaitingReset {
                node: NodeId::from_static("build"),
                at: yunta_testkit_core::fixed_now(),
            },
        },
    };
    let frame = standing(reset);
    assert!(attention(&frame, &RUN, true, &ascii()).is_none());
}

/// The design's run, as it is kept beside the application's tests.
fn designed() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../testkit-ui/fixtures/designed")
}

/// The design run's workflow, fragua's, as its manifest froze it: its reviewer's node fanned
/// out to a node a runner.
pub(super) fn fragua() -> yunta_core::Workflow {
    let frozen = std::fs::read(designed().join("run/manifest.yaml")).expect("its manifest");
    yunta_core::Manifest::read_frozen(&frozen)
        .expect("the design's manifest reads")
        .doc
        .workflow
}

/// The design's run cut at `seq`, as a person watches it while an engine drives it: its
/// frame read at its last event, its workflow and its state.
pub(super) fn designed_at(seq: u64) -> (RunFrame, yunta_core::Workflow, yunta_replay::RunState) {
    let engine = yunta_replay::registry::Engine::this(yunta_core::Pid::current());
    designed_under(seq, &engine)
}

/// The design's run cut at `seq` with no engine driving it, as a run parked on its person
/// stands: its frame read at its last event, its workflow and its state.
pub(super) fn designed_parked(
    seq: u64,
) -> (RunFrame, yunta_core::Workflow, yunta_replay::RunState) {
    designed_under(seq, &yunta_replay::registry::Engine::Unrecorded)
}

/// The design's run cut at `seq` under `engine`, read at its last event.
fn designed_under(
    seq: u64,
    engine: &yunta_replay::registry::Engine,
) -> (RunFrame, yunta_core::Workflow, yunta_replay::RunState) {
    let events: Vec<yunta_core::events::StoredEvent> =
        std::fs::read_to_string(designed().join("log.jsonl"))
            .expect("the design's run is kept")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a logged event"))
            .filter(|event: &yunta_core::events::StoredEvent| event.seq.get() <= seq)
            .collect();
    let state = yunta_replay::RunState::replay(&events);
    let now = state.clock().last_at().expect("a run with events");
    let workflow = fragua();
    let frame = RunFrame::of(&events[0].run_id, &workflow, &state, None, engine, now);
    (frame, workflow, state)
}

/// Fragua's stretches by the ribbon's rule, in the order its workflow declares its nodes.
const FRAGUA_NAMES: [&str; 15] = [
    "grill",
    "brief",
    "plan",
    "spec",
    "approve",
    "implement",
    "review",
    "alt",
    "conform",
    "fix",
    "lint",
    "fix-lint",
    "tests",
    "ship",
    "pr",
];

/// Each of fragua's stretches is named by the part of its id before `@`, else its first
/// word, where no stretch before it took that name; else its last word; else its whole id.
/// A workflow read with no run names them so, and so does a run of it.
#[test]
fn fraguas_stretches_are_named_by_the_rule_with_or_without_a_run() {
    let workflow = fragua();
    let ids: Vec<&str> = workflow.iter_nodes().map(|node| node.id.as_str()).collect();
    assert_eq!(stretch_names(&ids), FRAGUA_NAMES);
    let (frame, _, _) = designed_at(42);
    let ribbon = ribbon(&frame, Motion::Still);
    let named: Vec<&str> = ribbon
        .stretches
        .iter()
        .map(|stretch| stretch.name.as_str())
        .collect();
    assert_eq!(named, FRAGUA_NAMES);
}

/// `headline` as a terminal `cells` wide draws it, unpainted.
fn headline_drawn(headline: Headline, cells: usize) -> String {
    let drawn = Terminal::on(look(cells)).block(&Block::Headline(headline));
    drawn.iter().map(|row| Paint::Plain.line(row)).collect()
}

/// The headline across a run's screen says how long an open run has stood at its word, and
/// how long a closed one took, the handle at the far end; a run that stalled has stood there
/// as long, said as any open run's.
#[test]
fn the_headline_says_how_long_an_open_run_stood_and_how_long_a_closed_one_took() {
    let (at_work, _, _) = designed_at(42);
    let open = headline_drawn(headline(&at_work), 118);
    assert!(
        open.starts_with("● fragua (standard) · running · for 7m34s "),
        "{open}"
    );
    assert!(
        open.ends_with(" Q78GRR") && open.chars().count() == 118,
        "{open}"
    );
    let (finished, _, _) = designed_at(276);
    let closed = headline_drawn(headline(&finished), 118);
    assert!(
        closed.starts_with("✓ fragua (standard) · finished · in 1h52m "),
        "{closed}"
    );
    let stalled = RunFrame {
        standing: Standing::Stalled,
        phase: RunPhase::Running,
        elapsed: Some(Duration::from_secs(60)),
        since_last_event: Some(Duration::from_secs(9)),
        ..RunFrame::sample(&RUN)
    };
    let stalled = headline_drawn(headline(&stalled), 118);
    assert!(stalled.contains(" · stalled · for 9s "), "{stalled}");
}
