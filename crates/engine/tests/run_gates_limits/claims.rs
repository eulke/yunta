//! The claim an escalation makes is the engine's own for a gate that has no
//! `message:`, so a fact the gate shows cannot repeat it and end the run.

use yunta_core::events::{Claim, PauseReason};
use yunta_engine::RunTerminal;
use yunta_testkit::Bench;

/// A gate with no `message:` whose assignee is its own node id: the fact the
/// escalation shows is a word its default claim already carries.
const WORD_CLAIM_WORKFLOW: &str = r#"
name: word-claim
nodes:
  - id: plan
    kind: bash
    run: "true"
  - id: assignee-review
    kind: gate
    depends_on: [plan]
    assignee: assignee-review
    options: [proceed]
"#;

#[tokio::test]
async fn a_gate_whose_shown_fact_is_a_word_of_its_claim_waits_for_a_person() {
    let bench = Bench::new();
    let report = bench
        .try_run(WORD_CLAIM_WORKFLOW, "dialect: claude-code\nsessions: []\n")
        .await
        .expect("a gate with no surface pauses for a person; it does not end the run broken");

    match &report.terminal {
        RunTerminal::Paused {
            reason: PauseReason::Escalation(escalation),
        } => assert!(
            matches!(
                escalation.summary(),
                Claim::NeedsDecision { node } if node.as_str() == "assignee-review"
            ),
            "the gate asks what it needs, by its node: {:?}",
            escalation.summary()
        ),
        other => panic!("a gate with no surface must pause for a person, got {other:?}"),
    }
}
