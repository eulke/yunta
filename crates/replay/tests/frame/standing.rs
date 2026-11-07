//! The one word a run and each of its nodes are called by, decided once in
//! the frame: the row, the counter beside it and the document all read it.

use super::*;
use yunta_core::Pid;
use yunta_replay::registry::{EngineLiveness, Prompt};

/// What a run's registry says while its engine asks a person about `node`
/// at its terminal.
fn asking(node: &str) -> Engine {
    Engine {
        liveness: EngineLiveness::Alive,
        prompt: Some(Prompt {
            node: Some(node.into()),
            since: at(5),
            pid: Pid::try_from(4242).expect("a pid"),
        }),
        words: None,
        at: at(10),
    }
}

/// The run paused on `reason`.
fn paused(reason: PauseReason) -> EventPayload {
    EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(&reason)))
}

fn node<'a>(frame: &'a RunFrame, id: &str) -> &'a yunta_replay::frame::NodeFrame {
    frame
        .nodes
        .iter()
        .find(|node| node.id == id)
        .expect("declared")
}

#[test]
fn a_node_asked_about_at_a_terminal_needs_you_in_its_row_and_in_its_count() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (3, Some("build"), started(1)),
    ]);
    let frame = RunFrame::of(
        &RUN.into(),
        &chain(),
        &RunState::replay(&events),
        None,
        &asking("build"),
        at(10),
    );
    assert_eq!(node(&frame, "build").standing, Standing::NeedsYou);
    assert_eq!(frame.flow.of(Standing::NeedsYou), 1);
    assert_eq!(frame.flow.of(Standing::Running), 0);
    assert_eq!(frame.standing, Standing::NeedsYou);
}

#[test]
fn a_node_asked_about_before_its_log_starts_it_needs_you() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
    ]);
    let frame = RunFrame::of(
        &RUN.into(),
        &chain(),
        &RunState::replay(&events),
        None,
        &asking("build"),
        at(10),
    );
    assert_eq!(node(&frame, "build").standing, Standing::NeedsYou);
    assert_eq!(frame.flow.of(Standing::NeverRan), 1, "fix never ran");
}

#[test]
fn a_run_a_person_stopped_and_the_attempt_it_held_open_read_stopped() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, None, paused(PauseReason::Cancelled)),
    ]);
    let frame = frame(&chain(), &events, 10);
    assert_eq!(node(&frame, "plan").standing, Standing::Stopped);
    assert_eq!(frame.standing, Standing::Stopped);
    assert_eq!(buckets(&frame.flow), [0, 0, 0, 0, 1, 0, 2]);
}

#[test]
fn a_run_held_by_a_reset_alone_waits_and_so_does_the_node_it_holds() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), failed("the account's limit is reached")),
        (
            3,
            None,
            paused(PauseReason::AwaitingReset {
                node: "plan".into(),
                at: at(3_600),
            }),
        ),
    ]);
    let frame = frame(&chain(), &events, 10);
    assert_eq!(node(&frame, "plan").standing, Standing::Waiting);
    assert_eq!(frame.standing, Standing::Waiting);
}

#[test]
fn a_reset_with_a_node_waiting_on_a_person_needs_them() {
    let workflow = workflow(
        r#"
name: ship
nodes:
  - { id: approve, kind: gate, assignee: lead }
  - { id: build, kind: bash, run: "true" }
"#,
    );
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("approve"), started(1)),
        (2, Some("approve"), gate_waiting(None)),
        (3, Some("build"), started(1)),
        (4, Some("build"), failed("the account's limit is reached")),
        (
            5,
            None,
            paused(PauseReason::AwaitingReset {
                node: "build".into(),
                at: at(3_600),
            }),
        ),
    ]);
    let frame = frame(&workflow, &events, 10);
    assert_eq!(node(&frame, "approve").standing, Standing::NeedsYou);
    assert_eq!(node(&frame, "build").standing, Standing::Waiting);
    assert_eq!(
        frame.standing,
        Standing::NeedsYou,
        "a clock does not answer a gate"
    );
}

#[test]
fn a_moving_run_whose_engine_is_gone_is_stalled() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
    ]);
    let gone = Engine {
        liveness: EngineLiveness::Dead,
        prompt: None,
        words: None,
        at: at(10),
    };
    let frame = RunFrame::of(
        &RUN.into(),
        &chain(),
        &RunState::replay(&events),
        None,
        &gone,
        at(10),
    );
    assert_eq!(frame.standing, Standing::Stalled);
    assert_eq!(node(&frame, "plan").standing, Standing::Running);
}

/// The pause a gate nobody answered leaves on the log: the run's own, its
/// escalation naming the gate it asks for — the gate has written nothing.
fn parked_on_gate(gate: &str) -> EventPayload {
    let escalation = Escalation::new(
        Claim::Written("Approve the plan?".to_string()),
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: Some(gate.into()),
        }),
        NonEmpty::from((approve_option(), Vec::new())),
    );
    paused(PauseReason::Escalation(Box::new(escalation)))
}

#[test]
fn the_gate_a_run_is_parked_on_needs_you_before_it_writes_anything() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (3, None, parked_on_gate("build")),
    ]);
    let frame = frame(&chain(), &events, 10);
    assert_eq!(node(&frame, "build").standing, Standing::NeedsYou);
    assert!(
        matches!(
            &frame.phase,
            RunPhase::Waiting { on: WaitingOn::Node { node, .. } } if node.as_str() == "build"
        ),
        "{:?}",
        frame.phase
    );
    assert_eq!(frame.standing, Standing::NeedsYou);
    assert_eq!(frame.flow.of(Standing::NeverRan), 1, "fix never ran");
}

#[test]
fn a_gate_a_run_left_reads_as_never_run_again_once_the_run_closes() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (3, None, parked_on_gate("build")),
        (4, None, run_finished(TerminalState::Cancelled)),
    ]);
    let frame = frame(&chain(), &events, 10);
    assert_eq!(node(&frame, "build").standing, Standing::NeverRan);
}

#[test]
fn every_node_this_run_will_do_is_counted_once_under_its_word() {
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("plan"), started(1)),
        (2, Some("plan"), finished()),
        (3, Some("build"), started(1)),
    ]);
    let frame = frame(&chain(), &events, 10);
    let counted: usize = frame.flow.by.iter().map(|(_, count)| count).sum();
    assert_eq!(counted, frame.flow.total);
    for node in &frame.nodes {
        assert!(frame.flow.of(node.standing) > 0, "{} is counted", node.id);
    }
}
