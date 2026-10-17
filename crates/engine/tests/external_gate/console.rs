//! A console gate: a person's rejection fails the node with that
//! rejection, and a plan that cannot be proven comes with approval
//! withheld and the reason it cannot be.

use yunta_core::events::{
    Approval, Claim, EventPayload, Failure, GateEvent, GateWaitingPayload, HumanChoice,
    PauseReason, WhyWithheld,
};
use yunta_core::NodeId;
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction};

use super::{bench_on_a_forge, GATE_ONLY_WORKFLOW, NO_SESSIONS};
use crate::common::plan_session;
use crate::common::spec::{spec_of, specifying, GREETS};

#[tokio::test]
async fn a_console_gate_a_person_rejects_fails_with_the_rejection() {
    let bench = Bench::new();
    let person = ScriptedInteraction::new(HumanChoice {
        option: "reject".into(),
        by: "lead".into(),
        free_text: None,
    });

    let RunReport { terminal, state } = bench
        .run_with_interaction(GATE_ONLY_WORKFLOW, NO_SESSIONS, &person)
        .await;
    let failure = match terminal {
        RunTerminal::Paused {
            reason: PauseReason::NodeFailed { node, failure },
        } => {
            assert_eq!(node.as_str(), "approve");
            failure
        }
        other => panic!("expected Paused, got {other:?}"),
    };
    let rejected = Failure::Rejected {
        rejected_by: "lead".into(),
        on: Approval::Console,
    };
    assert_eq!(failure, rejected);
    assert_eq!(
        yunta_render::failure::sentence(&failure),
        "rejected from the console"
    );
    assert!(matches!(
        state.nodes.state("approve"),
        Some(NodeState::Failed { failure: recorded, .. }) if recorded == &rejected
    ));
}

/// The gate events of a bench's run that asked a person, in log order.
fn asked_payloads(bench: &Bench) -> Vec<GateWaitingPayload> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(payload))) => Some((**payload).clone()),
            _ => None,
        })
        .collect()
}

/// A planner that hands over a plan and then its spec, and a gate on
/// that plan with no forge reachable, so a person decides at the console.
const PLAN_GATE_AT_THE_CONSOLE: &str = r#"
name: plan-gate
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: spec
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [plan]
    prompt: "Write the tests the plan's tasks are held to."
    artifacts:
      produces: [spec]
  - id: approve-plan
    kind: gate
    depends_on: [spec]
    assignee: lead
    message: "Is this plan the one to build?"
    external:
      kind: pull_request
      artifacts: [tasks, spec]
      branch: "{{run.branch}}"
"#;

/// A plan whose task changes `tests/greet.sh`, the test its spec gives it.
const CHANGES_ITS_TEST: &str = r#"summary: "Greet"
description: "Writes the greeting the run is about."
tasks:
  - id: greet
    title: "Write the greeting"
    description: "Writes greeting.txt, and a check of it."
    scope: [greeting.txt, tests/greet.sh]
    changes:
      - { at: greeting.txt, what: "the greeting", code: "Hello" }
      - { at: tests/greet.sh, what: "a check of the greeting", code: "test -s greeting.txt" }
    outcome: "greeting.txt says hello"
    criteria:
      - { cmd: "test \"$(cat greeting.txt 2>/dev/null)\" = Hello", proves: "the greeting says hello" }
"#;

#[tokio::test]
async fn a_console_gate_withholds_approval_of_a_plan_that_cannot_be_proven_and_says_why() {
    let bench = Bench::new();
    let person = ScriptedInteraction::new(HumanChoice {
        option: "reject".into(),
        by: "lead".into(),
        free_text: None,
    });
    let spec = spec_of("greet", GREETS, "sh tests/greet.sh");
    let sessions = plan_session(CHANGES_ITS_TEST) + &specifying(&[(&spec, true)]);

    let RunReport { terminal, .. } = bench
        .run_with_interaction(PLAN_GATE_AT_THE_CONSOLE, &sessions, &person)
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let asked = asked_payloads(&bench)
        .into_iter()
        .next()
        .expect("the console asked about the plan");
    assert_eq!(
        asked.summary(),
        &Claim::ForgeUnreachable {
            node: NodeId::from_static("approve-plan"),
            assignee: "lead".to_string(),
        }
    );
    let withheld = asked.withheld();
    assert_eq!(withheld.len(), 1, "{withheld:?}");
    assert_eq!(withheld[0].option.as_str(), "approve");
    assert!(
        matches!(withheld[0].because, WhyWithheld::Unprovable { .. }),
        "the record holds the flaws that withhold the option: {withheld:?}"
    );
    assert_eq!(
        yunta_render::escalation::because(&withheld[0]),
        "the plan cannot be proven as it is written: `greet` plans to change `tests/greet.sh`, a test the spec wrote for `greet`"
    );
}

#[tokio::test]
async fn a_published_gate_checked_again_with_no_forge_records_that_as_its_claim() {
    let (bench, _forge_state) = bench_on_a_forge();
    bench.run(GATE_ONLY_WORKFLOW, NO_SESSIONS).await;

    let later = bench.without_forge();
    let person = ScriptedInteraction::new(HumanChoice {
        option: "reject".into(),
        by: "lead".into(),
        free_text: None,
    });
    later.wake_answering(&person).await;

    let [published, recheck]: [GateWaitingPayload; 2] = asked_payloads(&later)
        .try_into()
        .expect("one gate_waiting when the gate publishes, and one when it is checked again");
    assert_eq!(
        published.summary(),
        &Claim::AwaitsReview {
            node: NodeId::from_static("approve"),
            assignee: Some("reviewer".to_string()),
        }
    );
    let published_at = published
        .external_ref()
        .expect("the publish left its reference")
        .to_string();
    assert_eq!(
        recheck.summary(),
        &Claim::RecheckUnreachable {
            node: NodeId::from_static("approve"),
            external_ref: published_at,
        }
    );
}
