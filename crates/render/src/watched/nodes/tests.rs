use std::time::Duration;

use surco::line::Fill;
use surco::look::Look;
use surco::motion::Motion;
use surco::words::Standing;
use yunta_core::events::{Failure, NodeState, TokenUsage};
use yunta_core::{NodeId, OptionId};
use yunta_replay::frame::{NodeFrame, NodeStanding, NodeTimes, RunFrame, RunPhase, RunTimes};
use yunta_replay::RunState;

use super::nodes;
use crate::watched::tests::{ascii, designed_at, designed_parked, drawn, fragua, look, RUN};

/// The design's run cut at `seq`, its nodes as a run that leads draws them in `look`, the
/// pointer on `pointed`.
fn designed_rows(seq: u64, pointed: Option<&str>, look: &Look) -> Vec<String> {
    let (frame, times, workflow, state) = designed_at(seq);
    let pointed = pointed.map(NodeId::from);
    drawn(nodes(
        &frame,
        &times,
        &workflow,
        &state,
        pointed.as_ref(),
        true,
        look,
    ))
}

/// The design's run at the board: `implement` at work under the pointer, on the spinner's
/// frame of the run's last event, its task hanging under it at work; every node that
/// finished with how long it took, a gate with how long it waited.
#[test]
fn the_nodes_at_the_board_hang_the_loops_task_under_it_and_turn_on_it() {
    assert_eq!(
        designed_rows(99, Some("implement"), &look(31)),
        [
            "  ✓ grill                  36s",
            "  ✓ brief                  22s",
            "  ✓ plan                 6m25s",
            "  ✓ spec                13m53s",
            "  ✓ approve-plan        10m05s",
            "▸ ⠏ implement",
            "   └─ ● global-pack-install",
            "  ◌ review@reviewer",
            "  ◌ review@reviewer-alt",
            "  ◌ conform",
            "  ◌ fix-findings",
            "  ◌ lint",
            "  ◌ fix-lint",
            "  ◌ tests",
            "  ◌ ship",
            "  ◌ pr",
        ]
    );
    let ascii = designed_rows(99, Some("implement"), &ascii());
    assert_eq!(
        ascii[5..7],
        ["* \\ implement", "   `- > global-pack-install"]
    );
}

/// The reviewers' ids are as long as the name's column: each stands whole in it, every one
/// at work on the same frame, with how long it took after it once it finished.
#[test]
fn the_reviewers_keep_their_names_whole_in_the_column() {
    let at_work = designed_rows(200, Some("review@reviewer"), &look(31));
    assert_eq!(
        at_work[7..10],
        [
            "▸ ⠋ review@reviewer",
            "  ⠋ review@reviewer-alt",
            "  ⠋ conform"
        ]
    );
    let finished = designed_rows(276, None, &look(31));
    assert_eq!(
        finished[7..9],
        [
            "  ✓ review@reviewer      2m00s",
            "  ✓ review@reviewer-alt    42s"
        ]
    );
}

/// What the clock adds to `frame`, each of its nodes having worked `elapsed`.
fn worked(frame: &RunFrame, elapsed: Duration) -> RunTimes {
    let times = NodeTimes {
        elapsed: Some(elapsed),
        ..NodeTimes::sample()
    };
    let nodes = frame.nodes.iter().map(|node| (node.id.clone(), times));
    RunTimes {
        nodes: nodes.collect(),
        ..RunTimes::sample(frame)
    }
}

/// The rows of fragua's run `frame`, its nodes having worked `elapsed`, `pointed` pointed
/// at, the run leading where `leads`, drawn in `look`.
fn listed(
    frame: &RunFrame,
    elapsed: Duration,
    pointed: Option<&NodeId>,
    leads: bool,
    look: &Look,
) -> Vec<surco::line::Line> {
    let (times, state) = (worked(frame, elapsed), RunState::default());
    nodes(frame, &times, &fragua(), &state, pointed, leads, look)
}

/// A node at work under the pointer, and one a person's choice at its gate sent the run back
/// from, while the person leads: the pointed row filled with the selection, the one sent back
/// marked rerouted, and under the nodes a blank row, then the key that gives the lead back.
#[test]
fn while_the_person_leads_the_nodes_say_how_to_give_the_lead_back() {
    let mut plan = NodeFrame::sample(&NodeId::from("plan"), NodeStanding::ToGo);
    plan.standing = Standing::Running;
    let chose = Failure::GateChose {
        node: NodeId::from("approve-plan"),
        option: OptionId::from("adjust"),
        target: NodeId::from("plan"),
        said: None,
    };
    let failed = NodeState::Failed {
        failure: chose,
        tokens: TokenUsage::default(),
    };
    let approve = NodeId::from("approve-plan");
    let mut sent_back = NodeFrame::sample(&approve, NodeStanding::Reached(failed));
    sent_back.standing = Standing::Failed;
    let frame = RunFrame {
        phase: RunPhase::Running,
        standing: Standing::Running,
        nodes: vec![plan, sent_back],
        ..RunFrame::sample(&RUN)
    };
    let pointed = NodeId::from("plan");
    let rows = listed(&frame, Duration::ZERO, Some(&pointed), false, &look(31));
    assert_eq!(
        drawn(rows.clone()),
        [
            "▸ ⠋ plan",
            "  ↻ approve-plan",
            "",
            " f follow what it is doing"
        ]
    );
    assert_eq!(rows[0].fill(), Some(Fill::Selection));
    assert_eq!(rows[1].fill(), None);
}

/// How long a node took stands for one that finished or that a reroute corrects, never for
/// one that failed or stopped; a node at work in a run that does not move, as one whose
/// engine is gone, keeps its mark, as does every node on a look that stands still.
#[test]
fn a_node_says_how_long_it_took_once_done_and_turns_only_while_the_run_moves() {
    let done = |id: &'static str, standing: Standing| {
        let mut node = NodeFrame::sample(&NodeId::from_static(id), NodeStanding::ToGo);
        node.standing = standing;
        node
    };
    let stalled = RunFrame {
        phase: RunPhase::Running,
        standing: Standing::Stalled,
        nodes: vec![
            done("first", Standing::Rerouted),
            done("build", Standing::Failed),
            done("lint", Standing::Stopped),
            done("tests", Standing::Running),
        ],
        ..RunFrame::sample(&RUN)
    };
    let took = Duration::from_secs(45);
    let rows = drawn(listed(&stalled, took, None, true, &look(31)));
    assert_eq!(
        rows,
        [
            "  ↻ first                  45s",
            "  ✗ build",
            "  ■ lint",
            "  ● tests"
        ]
    );
    let moving = RunFrame {
        standing: Standing::Running,
        ..stalled
    };
    let still = Look {
        motion: Motion::Still,
        ..look(31)
    };
    let rows = drawn(listed(&moving, took, None, true, &still));
    assert_eq!(rows[3], "  ● tests");
}

/// A node whose failure the run is parked on for the person's decision draws the needs-you
/// mark, its log's failure aside, as the targets `scope`, `still-unread`, `node-needs-you`
/// and `card-words` draw `implement` (row 13) at the scope it asks for, and `failure` draws
/// `tests` (row 21) at its red suite; a loop's task stopped on its scope stays blocked.
#[test]
fn the_node_the_run_waits_on_a_person_at_draws_needs_you() {
    let rows = |seq| {
        let (frame, times, workflow, state) = designed_parked(seq);
        drawn(nodes(
            &frame,
            &times,
            &workflow,
            &state,
            None,
            true,
            &look(31),
        ))
    };
    let scope = rows(118);
    assert_eq!(
        scope[5..7],
        ["  ◆ implement", "   └─ ▲ global-pack-install"]
    );
    assert_eq!(rows(254)[13], "  ◆ tests");
}
