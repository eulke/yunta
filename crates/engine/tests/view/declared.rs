//! What a node's frame says beyond where it stands: what the frozen
//! workflow declares of it, how long and how often a person answered it,
//! and how many times its own rule rerouted it.

use super::*;
use yunta_engine::{Declares, GateAsks, NodeFrame};

/// A log of `(seconds since the run began, node, payload as the log
/// persists it)`.
fn wire(entries: &[(i64, Option<&str>, &str)]) -> Vec<StoredEvent> {
    entries
        .iter()
        .enumerate()
        .map(|(index, (offset, node, payload))| {
            let serde_json::Value::Object(object) = serde_json::from_str(payload).unwrap() else {
                panic!("a payload is an object: {payload}");
            };
            StoredEvent {
                run_id: RUN.into(),
                seq: (index as u64 + 1).into(),
                timestamp: at(*offset),
                node_id: node.map(Into::into),
                body: EventBody::from_object(object, 1).unwrap(),
            }
        })
        .collect()
}

const CREATED: &str = r#"{"kind":"run_created","manifest_hash":"03a119e94d983cbf5279c0959eca2fa361b3db64414f223eb0ec83f2cad41ae9","inputs":{},"mode":"alpha","base_branch":"main","base_commit":"d07a58e99f630cc0567076fa7d4cd19c7c6a3b49"}"#;
const STARTED: &str = r#"{"kind":"node_started","attempt":1}"#;
const FINISHED: &str =
    r#"{"kind":"node_finished","outcome":"ok","tokens_used":{"input":0,"output":0}}"#;
const ASKED: &str = r#"{"kind":"questions_asked","questions_hash":"7f6a1f4281c82bad69e3d2ecac0e87f6ebfb21f7f07f73e248d268210cc010ee","questions":["scope"],"tokens_used":{"input":0,"output":0}}"#;
const WAITING: &str = r#"{"kind":"gate_waiting","summary":"Approve?","evidence":[],"options":[{"id":"approve","label":"approve","tradeoff":"resolves this gate"}]}"#;
const RESOLVED: &str =
    r#"{"kind":"gate_resolved","chosen_option":"approve","resolved_by":"unverified:someone"}"#;
const ASKING: &str = r#"{"kind":"asking_opened"}"#;

/// The node `id` of `frame`.
fn node<'a>(frame: &'a RunFrame, id: &str) -> &'a yunta_engine::NodeFrame {
    frame
        .nodes
        .iter()
        .find(|node| node.id == id)
        .expect("the frozen dag declares it")
}

/// A gate waiting on a person's decision, the state a log leaves a gate in
/// while a person is asked about it. It has no forge handle to wait on.
fn gate_awaiting_a_person() -> NodeStanding {
    NodeStanding::Reached(NodeState::Waiting {
        on: NodeWait::Gate { external_ref: None },
    })
}

/// A node the log says finished. The tests that use it read a node's people
/// and rounds, which do not depend on how the node finished, so its outcome
/// and spend are fixed.
fn finished_node() -> NodeStanding {
    NodeStanding::Reached(NodeState::Finished {
        outcome: NodeOutcome::Summary("done".to_string()),
        tokens: TokenUsage::default(),
    })
}

/// A node's frame says what the frozen workflow declares of it before it
/// runs: where its failure goes, and what a gate will ask. A blank question
/// is none.
#[test]
fn a_frame_says_what_a_node_declares_before_it_runs() {
    let workflow = workflow(
        r#"
name: ship
nodes:
  - { id: lint, kind: bash, run: "true", on_failure: { goto: fix, max_reroutes: 1 } }
  - { id: fix, kind: bash, run: "true" }
  - { id: approve, kind: gate, assignee: lead, message: "Ship it?", depends_on: [lint] }
  - { id: check, kind: gate, assignee: lead, message: "  ", depends_on: [approve] }
"#,
    );
    let events = log(vec![(0, None, created("alpha"))]);
    let frame = frame(&workflow, &events, 10);

    let lint = &node(&frame, "lint").declares;
    let rule = lint
        .on_failure
        .as_ref()
        .expect("lint declares where its failure goes");
    assert_eq!(rule.goto, "fix");
    assert_eq!(rule.max_reroutes, 1);
    assert_eq!(lint.gate, None);
    assert_eq!(
        node(&frame, "approve").declares.gate,
        Some(GateAsks {
            question: Some("Ship it?".to_string()),
            on_forge: false,
        })
    );
    assert_eq!(
        node(&frame, "check").declares.gate,
        Some(GateAsks {
            question: None,
            on_forge: false,
        })
    );
    assert_eq!(node(&frame, "fix").declares, Declares::default());
}

/// A node's frame holds how long a person was asked about it, the wait
/// open at the frame's instant included, how often a person decided its
/// gate, and how many rounds of questions it asked. Two nodes asked at once
/// each hold their own wait.
#[test]
fn a_frame_holds_how_long_and_how_often_a_person_answered_a_node() {
    let workflow = workflow(
        r#"
name: ship
nodes:
  - { id: approve, kind: gate, assignee: lead }
  - { id: ship, kind: gate, assignee: lead }
  - { id: ask, kind: bash, run: "true" }
"#,
    );
    let events = wire(&[
        (0, None, CREATED),
        (10, Some("approve"), ASKING),
        (40, Some("approve"), STARTED),
        (40, Some("approve"), WAITING),
        (40, Some("approve"), RESOLVED),
        (40, Some("approve"), FINISHED),
        (60, Some("ship"), ASKING),
        (70, Some("ask"), STARTED),
        (75, Some("ask"), ASKED),
    ]);
    let frame = frame(&workflow, &events, 100);

    let approve = node(&frame, "approve");
    assert_eq!(approve.people, Duration::from_secs(30));
    assert_eq!(approve.decisions, 1);
    assert_eq!(approve.question_rounds, 0);
    let ship = node(&frame, "ship");
    assert_eq!(ship.people, Duration::from_secs(40));
    assert_eq!(ship.decisions, 0);
    let ask = node(&frame, "ask");
    assert_eq!(ask.people, Duration::from_secs(25));
    assert_eq!(ask.question_rounds, 1);
}

/// A node's frame counts the reroutes its own `on_failure` made.
#[test]
fn a_frame_counts_the_reroutes_a_node_s_own_rule_made() {
    let workflow = chain();
    let events = log(vec![
        (0, None, created("alpha")),
        (1, Some("build"), started(1)),
        (2, Some("build"), failed("red")),
        (3, Some("build"), rerouted("fix")),
    ]);
    let frame = frame(&workflow, &events, 100);

    assert_eq!(node(&frame, "build").reroutes, 1);
    assert_eq!(node(&frame, "plan").reroutes, 0);
}

/// A gate's time is a person's once a person was asked about it; no other
/// node's is.
#[test]
fn a_gate_a_person_was_asked_about_took_a_persons_time() {
    let id = NodeId::from("approve");
    let gate = Declares {
        gate: Some(GateAsks {
            question: None,
            on_forge: false,
        }),
        ..Declares::default()
    };
    let asked = NodeFrame {
        people: Duration::from_secs(60),
        declares: gate.clone(),
        ..yunta_testkit::node_frame(&id, gate_awaiting_a_person())
    };
    assert!(asked.person_took());

    let unasked = NodeFrame {
        people: Duration::ZERO,
        declares: gate,
        ..yunta_testkit::node_frame(&id, gate_awaiting_a_person())
    };
    assert!(!unasked.person_took());

    let not_a_gate = NodeFrame {
        people: Duration::from_secs(60),
        ..yunta_testkit::node_frame(&NodeId::from("ask"), finished_node())
    };
    assert!(!not_a_gate.person_took());
}

/// A gate's rounds are its decisions and the round it waits in now.
#[test]
fn a_gate_counts_the_round_it_waits_in() {
    let id = NodeId::from("approve");
    let gate = Declares {
        gate: Some(GateAsks {
            question: None,
            on_forge: false,
        }),
        ..Declares::default()
    };
    let waiting = NodeFrame {
        decisions: 2,
        standing: Standing::NeedsYou,
        declares: gate.clone(),
        ..yunta_testkit::node_frame(&id, gate_awaiting_a_person())
    };
    assert_eq!(waiting.person_rounds(), 3);

    let decided = NodeFrame {
        decisions: 2,
        standing: Standing::Finished,
        declares: gate,
        ..yunta_testkit::node_frame(&id, finished_node())
    };
    assert_eq!(decided.person_rounds(), 2);
}

/// A node that is not a gate counts its rounds of questions.
#[test]
fn a_node_counts_its_rounds_of_questions() {
    let frame = NodeFrame {
        question_rounds: 2,
        decisions: 0,
        ..yunta_testkit::node_frame(&NodeId::from("ask"), finished_node())
    };
    assert_eq!(frame.person_rounds(), 2);
}
