//! The order a task's criteria run in, read from the run's own log.
//!
//! A pre-check runs every criterion of its task, cheapest first: the
//! fast commands land their evidence before the expensive suite. What
//! each command costs is a fact `criteria_checked` records, so the
//! order is derived from the log like every other piece of a run's
//! state — a wake that resumes a run inherits what the wakes before it
//! measured instead of paying for the measurement again.

use tokio_util::sync::CancellationToken;
use yunta_core::events::{CriteriaCheckedPayload, EventPayload, NodeEvent, Phase, SessionEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{wait_until_async, Bench};

/// The cheapest criterion, and a guard both tasks share: a process and
/// nothing else.
const CHEAP_GUARD: &str = "true";
/// A shared guard, distinct from the cheapest guard and with no delay.
const MIDDLE_GUARD: &str = "test -e .git";
/// The first task's own criterion: red until the task's file exists.
const FIRST_TASK_CRITERION: &str = "test -f hello.txt";
/// The second task's, the same shape.
const SECOND_TASK_CRITERION: &str = "test -f world.txt";

/// A plan node that hands over the tasks document, and the loop that
/// works it task by task.
const WORKFLOW: &str = r#"
name: learned-order
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// The planner hands over two tasks whose criteria are declared
/// most-expensive-first, the reverse of what they cost; the first task's
/// session does its work, and the second's hangs, so the run is still
/// open when the cancellation reaches it.
fn plans_then_hangs() -> String {
    format!(
        "\
capabilities:
  run_tools: true
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            tasks:
              - id: T001
                title: Create hello
                scope: [hello.txt]
                criteria:
                  - cmd: \"{FIRST_TASK_CRITERION}\"
                  - cmd: \"{MIDDLE_GUARD}\"
                    type: guard
                  - cmd: \"{CHEAP_GUARD}\"
                    type: guard
              - id: T002
                title: Create world
                scope: [world.txt]
                depends_on: [T001]
                criteria:
                  - cmd: \"{SECOND_TASK_CRITERION}\"
                  - cmd: \"{MIDDLE_GUARD}\"
                    type: guard
                  - cmd: \"{CHEAP_GUARD}\"
                    type: guard
    outcome:
      type: completed
      summary: planned
  - effects:
      - path: hello.txt
        content: hello
    outcome:
      type: completed
      summary: did T001
  - outcome:
      type: hang
"
    )
}

/// What the resumed run's one session does: the work the second task's
/// criteria are about.
const FINISHES_THE_SECOND_TASK: &str = "\
sessions:
  - effects:
      - path: world.txt
        content: world
    outcome:
      type: completed
      summary: did T002
";

/// Fires `token` once the second task's session is open — by then its
/// pre-check has run and is on the log, and the hanging session holds
/// the run there until the cancellation lands. Waiting on what the log
/// says, never on a duration, is what makes the moment exact.
async fn cancel_once_the_second_task_holds_a_session(bench: &Bench, token: &CancellationToken) {
    wait_until_async(
        || async move { sessions_opened(bench) == 3 },
        || {
            let events = bench.events();
            format!(
                "the second task never opened its session ({} did); events: {events:#?}",
                sessions_opened(bench),
            )
        },
    )
    .await;
    token.cancel();
}

/// How many sessions the run has opened: the planner's, and one per
/// task dispatched.
fn sessions_opened(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(_)))
            )
        })
        .count()
}

/// Every pre-check the log records for `task`, oldest first, as the
/// commands it ran in the order it ran them.
fn pre_checks_of(bench: &Bench, task: &str) -> Vec<Vec<String>> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
                task_id,
                phase: Phase::Pre,
                results,
            }))) if task_id.as_str() == task => {
                Some(results.iter().map(|result| result.cmd.clone()).collect())
            }
            _ => None,
        })
        .collect()
}

/// A run cut in the middle of its work, whose log holds what each of
/// its criteria cost: the first task ran all three commands and
/// finished, and the second's session was cancelled after its own
/// pre-check had paid for its own criterion. The bench it answers with
/// has no cancellation standing, so the next wake is an ordinary
/// resume.
async fn a_run_whose_log_priced_its_criteria() -> Bench {
    let token = CancellationToken::new();
    let bench = Bench::new().with_cancel(token.clone());
    let fixture = plans_then_hangs();
    let (first, ()) = tokio::join!(
        bench.run(WORKFLOW, &fixture),
        cancel_once_the_second_task_holds_a_session(&bench, &token)
    );
    assert!(
        matches!(first.terminal, RunTerminal::Paused { .. }),
        "the cancelled run pauses: {:?}",
        first.terminal
    );
    assert_eq!(
        pre_checks_of(&bench, "T001").first(),
        Some(&vec![
            FIRST_TASK_CRITERION.to_string(),
            MIDDLE_GUARD.to_string(),
            CHEAP_GUARD.to_string(),
        ]),
        "the first task meets its criteria as declared, off a log that prices nothing"
    );
    bench.with_cancel(CancellationToken::new())
}

/// The median of the durations the pre-resume event log actually stores.
fn median(history: &[yunta_core::events::StoredEvent], cmd: &str) -> Option<u64> {
    let mut samples: Vec<f64> = history
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(payload))) => Some(payload),
            _ => None,
        })
        .flat_map(|payload| payload.results.iter())
        .filter(|result| result.cmd == cmd)
        .filter_map(|result| result.duration_ms)
        .map(|duration_ms| duration_ms as f64)
        .collect();
    if samples.is_empty() {
        return None;
    }
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    let median = if samples.len() % 2 == 1 {
        samples[middle]
    } else {
        (samples[middle - 1] + samples[middle]) / 2.0
    };
    Some(median as u64)
}

#[tokio::test]
async fn a_resumed_pre_check_runs_criteria_in_the_order_the_log_priced_them() {
    let bench = a_run_whose_log_priced_its_criteria().await;

    // The order is judged against the evidence already on disk when the
    // resume begins, never against guessed command runtimes.
    let history = bench.events();
    let declared = [SECOND_TASK_CRITERION, MIDDLE_GUARD, CHEAP_GUARD];
    let mut expected: Vec<(usize, &str)> = declared.iter().copied().enumerate().collect();
    expected.sort_by_key(|(index, cmd)| (median(&history, cmd).unwrap_or(u64::MAX), *index));
    let expected: Vec<String> = expected
        .into_iter()
        .map(|(_, cmd)| cmd.to_string())
        .collect();

    // The resume is a separate execution: nothing carries over but the
    // log, which now says what each command cost.
    let RunReport { terminal, state } = bench.wake_on_fixture(FINISHES_THE_SECOND_TASK).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("T002"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    let resumed = pre_checks_of(&bench, "T002");
    let actual = resumed.last().expect("the resumed pre-check is in the log");
    assert_eq!(
        actual, &expected,
        "the resume sorts by logged median; equal medians retain declaration order"
    );
    assert_eq!(
        actual.len(),
        declared.len(),
        "the pre-check evaluates the complete set"
    );
    for cmd in declared {
        assert_eq!(
            actual.iter().filter(|seen| seen.as_str() == cmd).count(),
            1,
            "`{cmd}` runs exactly once: {actual:?}"
        );
    }
}
