use yunta_core::events::ChildLink;
use yunta_core::events::NodeState;
use yunta_core::NodeId;
use yunta_replay::frame::{Counter, NodeStanding, RunPhase, WaitingOn};
use yunta_replay::frame::{NodeFrame, RunFrame};
use yunta_testkit_core::Captured;

use crate::surface::Watched;

use super::*;

const RUN: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P5");
const CHILD: RunId = RunId::from_static("01JBZ5X8K3N7Q2W6E4R9T1Y0P6");

/// A region drawn into a buffer: no terminal to stand up, and every
/// draw landing before the next assertion.
fn region(term: &Watched, scrollback: &Captured) -> Region {
    Region::open(
        Screen::immediate(term.clone()),
        Glyphs::Ascii,
        surco::paint::Paint::Plain,
        Box::new(scrollback.clone()),
    )
    .expect("the region's row template parses")
}

fn running(id: &'static str) -> yunta_replay::frame::NodeFrame {
    NodeFrame::sample(
        &NodeId::from_static(id),
        NodeStanding::Reached(NodeState::Running { attempt: 1 }),
    )
}

/// A run of `nodes` in `phase`, counted as the region counts them.
fn frame(phase: RunPhase, nodes: Vec<yunta_replay::frame::NodeFrame>) -> RunFrame {
    let engine = yunta_replay::registry::Engine::this(yunta_core::Pid::current());
    let standing = yunta_replay::frame::run_standing(&phase, &nodes, 0, &engine);
    RunFrame {
        standing,
        phase,
        flow: Counter {
            total: nodes.len(),
            ..Counter::default()
        },
        nodes,
        ..RunFrame::sample(&RUN)
    }
}

/// The run of `nodes`, with a child run still open under `bore`.
fn composing(bore: &'static str, nodes: Vec<yunta_replay::frame::NodeFrame>) -> RunFrame {
    RunFrame {
        children: vec![ChildLink::sample(
            &CHILD,
            Some(&NodeId::from_static(bore)),
            None,
        )],
        ..frame(RunPhase::Running, nodes)
    }
}

fn rows(term: &Watched) -> Vec<String> {
    term.shown().lines().map(str::to_string).collect()
}

/// A run stopped at the gate `plan`, for a person to answer.
fn parked_on_plan() -> RunFrame {
    frame(
        RunPhase::Waiting {
            on: WaitingOn::Node {
                node: NodeId::from_static("plan"),
                on: yunta_core::events::NodeWait::Gate { external_ref: None },
                reason: None,
            },
        },
        vec![running("plan")],
    )
}

#[test]
fn the_attention_row_opens_the_region_only_while_something_needs_you() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);

    region.show(
        &frame(RunPhase::Running, vec![running("plan")]),
        &RUN,
        false,
    );
    let quiet = rows(&term);
    assert!(
        quiet
            .first()
            .is_some_and(|row| row.contains("plan running")),
        "with nothing to ask, the work opens the region: {quiet:?}"
    );
    assert!(
        quiet
            .last()
            .is_some_and(|row| row.ends_with("nothing needs you")),
        "and the footer says nothing needs you: {quiet:?}"
    );

    region.show(&parked_on_plan(), &RUN, true);
    let demanded = rows(&term);
    let first = demanded.first().map(String::as_str).unwrap_or_default();
    assert!(
        first.contains("needs you: ")
            && first.contains(&format!("yunta resolve-gate {}", RUN.handle())),
        "the row that asks opens the region, with the command that answers: {demanded:?}"
    );
    assert_eq!(demanded.len(), quiet.len() + 1, "{demanded:?}");

    region.show(
        &frame(RunPhase::Running, vec![running("plan")]),
        &RUN,
        false,
    );
    assert_eq!(rows(&term), quiet, "and it leaves once it is answered");
}

#[test]
fn a_closed_region_leaves_nothing_of_itself_on_the_terminal() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);
    region.show(
        &frame(RunPhase::Running, vec![running("plan")]),
        &RUN,
        false,
    );
    assert!(!term.shown().is_empty(), "there is a region to take down");

    let drawn = term.written();
    region.close();
    assert_eq!(
        term.shown(),
        "",
        "the block that closes the run out lands on a terminal with nothing pinned to it"
    );
    assert_eq!(
        term.written(),
        drawn,
        "and nothing of the region reaches that terminal after it is taken down"
    );
}

#[test]
fn a_row_is_cut_to_the_terminal_and_never_past_the_width_it_is_written_for() {
    // Below the terminal's edge a row wraps onto the row below,
    // which the next redraw does not account for: the region loses
    // its row count and tears. Above the width the rows are written
    // for, the layout would reflow under a reader every time they
    // resized the window.
    const PARKED: &str = "a-node-with-a-name-long-enough-to-fill-a-row";
    for columns in [40u16, 200] {
        let term = Watched::sized(16, columns);
        let scrollback = Captured::default();
        let mut region = region(&term, &scrollback);
        region.show(
            &frame(
                RunPhase::Waiting {
                    on: WaitingOn::Node {
                        node: NodeId::from_static(PARKED),
                        on: yunta_core::events::NodeWait::Gate { external_ref: None },
                        reason: None,
                    },
                },
                vec![running(PARKED)],
            ),
            &RUN,
            true,
        );
        let room = crate::output::row_width(usize::from(columns)).cells();
        let drawn = rows(&term);
        assert_eq!(
            drawn.len(),
            2 + region.body.len(),
            "on a terminal {columns} wide the region took more rows than it drew, which is \
             a row that wrapped onto the one below: {drawn:?}"
        );
        for row in &drawn {
            assert!(
                surco::width::cell_width(row) <= room,
                "a row {} cells wide where {room} fit, on a terminal {columns} wide: {row:?}",
                surco::width::cell_width(row)
            );
        }
        assert!(
            term.shown().contains("needs you"),
            "and the rows are still the run's: {}",
            term.shown()
        );
    }
}

#[test]
fn a_child_run_is_drawn_under_the_node_that_bore_it_and_never_as_a_number() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);

    region.show(&composing("compose", vec![running("compose")]), &RUN, false);
    let drawn = rows(&term);
    let node = drawn
        .iter()
        .position(|row| row.contains("compose"))
        .expect("the node that bore the child is on the terminal");
    let child = drawn
        .get(node + 1)
        .map(String::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(
        child.starts_with("    ") && child.contains(&format!("child run {}", CHILD.handle())),
        "the child sits under the node that bore it: {drawn:?}"
    );
    assert!(
        child.contains("still open"),
        "and says only what the parent's log says of it: {child}"
    );
    assert!(
        !child.contains('%'),
        "children are a tree, never a figure averaged over them: {child}"
    );
}

#[test]
fn a_child_row_cut_to_a_narrow_terminal_still_says_where_the_child_stands() {
    const NARROW: u16 = 40;
    let term = Watched::sized(16, NARROW);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);

    region.show(&composing("compose", vec![running("compose")]), &RUN, false);
    let drawn = rows(&term);
    assert!(
        drawn.iter().any(|row| row.contains("still open")),
        "a row cut at the terminal's edge loses which child and never that there is \
         one: {drawn:?}"
    );
    for row in &drawn {
        assert!(
            surco::width::cell_width(row) <= usize::from(NARROW),
            "{row:?}"
        );
    }
}

#[test]
fn the_footer_stays_the_last_row_as_nodes_come_and_go() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);

    for nodes in [
        vec![running("plan")],
        vec![running("plan"), running("build")],
        vec![running("build")],
    ] {
        region.show(&frame(RunPhase::Running, nodes), &RUN, false);
        let drawn = rows(&term);
        assert!(
            drawn
                .last()
                .is_some_and(|row| row.contains(" of ") && row.contains(" done")),
            "{drawn:?}"
        );
    }
}

/// What a session of `node` in `run` said.
fn said(run: &RunId, node: &'static str, text: &str) -> yunta_core::port::Said {
    yunta_core::port::Said {
        run_id: run.clone(),
        node_id: NodeId::from_static(node),
        session_id: yunta_core::SessionId::from_static("session-1"),
        task_id: None,
        seq: yunta_core::Seq::from(3),
        at: chrono::DateTime::UNIX_EPOCH,
        text: text.to_string(),
        cut: false,
    }
}

/// A node shows what its session said while it works. Once it stops, what
/// it said is let go, so working again it shows nothing until it says
/// something new.
#[test]
fn a_node_that_works_again_does_not_show_what_its_last_attempt_said() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);
    region.hear(said(&RUN, "build", "Compiling the parser."));

    region.show(
        &frame(RunPhase::Running, vec![running("build")]),
        &RUN,
        false,
    );
    assert!(
        rows(&term)
            .iter()
            .any(|row| row.contains("says: Compiling the parser.")),
        "{:?}",
        rows(&term)
    );

    region.show(
        &frame(RunPhase::Running, vec![running("test")]),
        &RUN,
        false,
    );
    region.show(
        &frame(RunPhase::Running, vec![running("build")]),
        &RUN,
        false,
    );
    assert!(
        !rows(&term).iter().any(|row| row.contains("says:")),
        "{:?}",
        rows(&term)
    );
}

/// A follower hears a session off the run's socket and reads its node's start
/// off the log, and the words can come first. What a node that has not started
/// yet said is kept, and shown once the node works.
#[test]
fn what_a_node_said_before_its_start_is_shown_once_it_works() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);
    region.hear(said(&RUN, "build", "First I read the parser."));

    let to_go = NodeFrame::sample(&NodeId::from_static("build"), NodeStanding::ToGo);
    region.show(&frame(RunPhase::Running, vec![to_go]), &RUN, false);
    region.show(
        &frame(RunPhase::Running, vec![running("build")]),
        &RUN,
        false,
    );

    assert!(
        rows(&term)
            .iter()
            .any(|row| row.contains("says: First I read the parser.")),
        "{:?}",
        rows(&term)
    );
}

/// What a session of another run said, such as a child's, is not shown in
/// this run's region, even under a node of the same name.
#[test]
fn what_another_run_said_is_not_shown() {
    let term = Watched::sized(16, 80);
    let scrollback = Captured::default();
    let mut region = region(&term, &scrollback);
    region.hear(said(&CHILD, "build", "Said in the child."));

    region.show(
        &frame(RunPhase::Running, vec![running("build")]),
        &RUN,
        false,
    );

    assert!(
        !rows(&term).iter().any(|row| row.contains("says:")),
        "{:?}",
        rows(&term)
    );
}
