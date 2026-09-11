//! The repair cycle: an interpreted artifact that could not be read
//! reopens a session with the diagnostics instead of ending the node.
//!
//! This was the only frontier of the engine without a cycle. Work has
//! had one from the start — red criterion, session, green criterion,
//! retry, escalation — while a declaration that was one key off threw
//! away the whole session that produced it and waited for a person.

use yunta_core::events::EventPayload;
use yunta_engine::RunTerminal;
use yunta_testkit::Bench;

const PLAN_ONLY: &str = r#"
name: plan-only
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger."
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
"#;

/// What an agent writes when it has never seen the shape: a key that
/// reads right and a criterion as plain text.
const WRONG_LEDGER: &str = "tasks:\\n  - id: t1\\n    title: Work\\n    description: the toggle\\n    scope: [\\\"src/**\\\"]\\n    criteria:\\n      - cargo test\\n";

const RIGHT_LEDGER: &str =
    "tasks:\\n  - id: t1\\n    title: Work\\n    scope: [\\\"src/**\\\"]\\n    criteria:\\n      - cmd: \\\"cargo test\\\"\\n";

fn plan_path(bench: &Bench) -> String {
    bench
        .run_dir()
        .join("artifacts")
        .join("plan.yaml")
        .display()
        .to_string()
}

/// Every attempt this node announced.
fn attempts(events: &[yunta_core::events::StoredEvent], node: &str) -> usize {
    events
        .iter()
        .filter(|e| e.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter(|e| matches!(e.payload(), Some(EventPayload::NodeStarted(_))))
        .count()
}

#[tokio::test]
async fn a_ledger_that_could_not_be_read_is_written_again_and_the_node_finishes() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    // The second script only matches a prompt carrying the diagnostics,
    // so the run finishing at all proves they reached the session.
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n  \
         - match_prompt_contains: \"could not be read\"\n    \
           effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: repaired }}\n"
    );

    let (terminal, state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert!(
        state.tasks.keys().any(|id| id.as_str() == "t1"),
        "the repaired ledger registered its task: {state:?}"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        2,
        "a repair is a fresh attempt on the log, visible in status and stats"
    );
}

#[tokio::test]
async fn the_repair_attempt_is_told_what_was_wrong_and_the_shape_to_write() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    // Three needles the first prompt cannot contain: the problem named
    // in the document's own vocabulary, the key that replaces the one
    // the agent invented, and the published shape.
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n  \
         - match_prompt_contains: \"task `t1`, criterion 1\"\n    \
           effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: repaired }}\n"
    );
    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_node_that_never_gets_it_right_fails_once_the_budget_is_spent() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    // Both scripts write the same unreadable file. The default budget is
    // one repair, so exactly two sessions run and then the node fails.
    let script = format!(
        "  - effects:\n      - {{ path: \"{path}\", content: \"{WRONG_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );
    let fixture = format!("sessions:\n{script}{script}");

    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the budget is a budget: {terminal:?}"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(attempts(&events, "plan"), 2, "one attempt, then one repair");

    // The failure a person reads still carries every violation, and the
    // diagnostics behind it survive on the log as data.
    let failure = events
        .iter()
        .rev()
        .find_map(|e| match e.payload() {
            Some(EventPayload::NodeFailed(p)) => Some(p),
            _ => None,
        })
        .expect("the node failed");
    assert!(failure.outcome.contains("unknown key `description`"));
    assert!(
        failure
            .diagnostics
            .iter()
            .any(|d| d.code() == "unknown-key"),
        "{:?}",
        failure.diagnostics
    );
}

#[tokio::test]
async fn an_artifact_that_was_never_written_does_not_enter_the_cycle() {
    let bench = Bench::new();
    // Nothing to correct: the session wrote no file at all. Asking for
    // it again is a different question from asking for it in the right
    // shape, and the ledger's own budget is not the place to answer it.
    let fixture = "sessions:\n  - outcome: { type: completed, summary: planned }\n";

    let (terminal, _state) = bench.run(PLAN_ONLY, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        1,
        "a file that does not exist is not a file to rewrite"
    );
}

#[tokio::test]
async fn a_node_whose_artifact_reads_first_time_runs_exactly_one_session() {
    let bench = Bench::new();
    let path = plan_path(&bench);
    let fixture = format!(
        "sessions:\n  \
         - effects:\n      - {{ path: \"{path}\", content: \"{RIGHT_LEDGER}\" }}\n    \
           outcome: {{ type: completed, summary: planned }}\n"
    );

    let (terminal, _state) = bench.run(PLAN_ONLY, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        attempts(&events, "plan"),
        1,
        "the cycle costs nothing when nothing is wrong"
    );
}
