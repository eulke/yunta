//! A node that fails with no `on_failure` re-route, in a run that pauses
//! on failures, is a decision: run it again or stop. A person who fixes
//! the cause hands the node back with `retry` — at the terminal while
//! the run asks, or later through `resolve_gate` — and a plain resume
//! never spends on a retry nobody chose.

use yunta_core::events::{GateEvent, NodeEvent};
use yunta_engine::{current_escalation, RunReport, RunTerminal};
use yunta_testkit::{write, Bench};

mod common;
use common::*;

/// A node with no `on_failure` whose run pauses on failures: it fails
/// until `fixed.txt` exists, which only a person puts there.
const FAILS_UNTIL_FIXED_WORKFLOW: &str = r#"
name: plain-failure
nodes:
  - id: broken
    kind: bash
    run: "test -f fixed.txt"
"#;

/// How many attempts of `node` the run started.
fn attempts(bench: &Bench, node: &str) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| {
            e.node_id.as_ref().is_some_and(|id| id.as_str() == node)
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Node(NodeEvent::Started(
                        _
                    )))
                )
        })
        .count()
}

#[tokio::test]
async fn a_plain_failure_reconstructs_a_retry_and_abort_escalation() {
    // A failure with nothing to re-route it is still a decision: a person
    // who fixes its cause needs a way to hand the node back, and a resume
    // alone would find it failed and pause again.
    let bench = parked(FAILS_UNTIL_FIXED_WORKFLOW, "sessions: []\n").await;
    let (node, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events()))
            .expect("a failed node is a pause with a menu");
    assert_eq!(node.as_str(), "broken");
    assert_eq!(escalation.summary(), "node `broken` failed");
    let ids: Vec<&str> = escalation.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["retry", "abort"]);
    assert_eq!(
        escalation.options()[0].label,
        "Run `broken` again (attempt 2)"
    );
}

#[tokio::test]
async fn a_plain_failure_retried_after_its_cause_is_fixed_finishes() {
    let bench = parked(FAILS_UNTIL_FIXED_WORKFLOW, "sessions: []\n").await;
    write(&bench.worktree.join("fixed.txt"), "fixed");

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, state } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("broken"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    assert_eq!(attempts(&bench, "broken"), 2);
}

#[tokio::test]
async fn a_plain_resume_never_retries_a_failed_node_by_itself() {
    // Fixing the cause is not the decision to spend again: without a
    // `retry` on the log, a resume parks on the same failure.
    let bench = parked(FAILS_UNTIL_FIXED_WORKFLOW, "sessions: []\n").await;
    write(&bench.worktree.join("fixed.txt"), "fixed");

    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the resume to pause again, got {terminal:?}");
    };
    assert!(reason.starts_with("node `broken` failed"), "{reason}");
    assert_eq!(attempts(&bench, "broken"), 1);
}

#[tokio::test]
async fn a_retry_that_fails_again_asks_again_for_the_next_attempt() {
    let bench = parked(FAILS_UNTIL_FIXED_WORKFLOW, "sessions: []\n").await;

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        attempts(&bench, "broken"),
        2,
        "one retry runs the node once"
    );
    let (_, escalation) =
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events()))
            .expect("the second failure is a decision too");
    assert_eq!(
        escalation.options()[0].label,
        "Run `broken` again (attempt 3)"
    );
}

#[tokio::test]
async fn a_person_who_fixes_the_cause_while_asked_retries_in_the_same_invocation() {
    // The live path: the question stands while the person repairs what
    // the node failed on, and answering `retry` continues this very run.
    struct FixThenRetry {
        fix: std::path::PathBuf,
    }
    #[async_trait::async_trait]
    impl yunta_engine::HumanInteraction for FixThenRetry {
        async fn resolve(
            &self,
            escalation: &yunta_core::events::GateWaitingPayload,
        ) -> Option<yunta_core::events::HumanChoice> {
            assert_eq!(escalation.summary(), "node `broken` failed");
            write(&self.fix, "fixed");
            Some(yunta_core::events::HumanChoice {
                option: "retry".into(),
                by: "lead".into(),
                free_text: None,
            })
        }
    }

    let bench = Bench::new();
    let fix = FixThenRetry {
        fix: bench.worktree.join("fixed.txt"),
    };
    let RunReport { terminal, state } = bench
        .run_with_interaction(FAILS_UNTIL_FIXED_WORKFLOW, "sessions: []\n", &fix)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.state("broken"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    assert_eq!(attempts(&bench, "broken"), 2);
    assert_eq!(
        events_matching(&bench, |p| matches!(
            p,
            yunta_core::events::EventPayload::Gates(GateEvent::Resolved(_))
        )),
        1,
        "the decision is recorded once, as the answer it was"
    );
}

#[tokio::test]
async fn aborting_a_failed_node_pauses_on_the_failure_and_asks_again_on_resume() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(
            FAILS_UNTIL_FIXED_WORKFLOW,
            "sessions: []\n",
            &SequencedInteraction::choosing(&["abort"]),
        )
        .await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected the abort to pause, got {terminal:?}");
    };
    assert!(reason.starts_with("node `broken` failed"), "{reason}");

    // The abort was consumed by its own pause: a later resume asks again
    // rather than re-applying it, and the menu is still there to answer.
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert!(
        current_escalation(&bench.manifest(), &yunta_engine::derive(&bench.events())).is_some()
    );
    assert_eq!(attempts(&bench, "broken"), 1);
}
