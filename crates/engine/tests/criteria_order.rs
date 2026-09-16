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
/// The other shared guard, an order of magnitude above the cheapest.
const MIDDLE_GUARD: &str = "sleep 0.05; true";
/// The first task's own criterion, and the most expensive of the three
/// it runs: red until the task's file exists, green after.
const FIRST_TASK_CRITERION: &str = "sleep 0.2; test -f hello.txt";
/// The second task's, the same cost and the same shape.
const SECOND_TASK_CRITERION: &str = "sleep 0.2; test -f world.txt";

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
            format!(
                "the second task never opened its session ({} did)",
                sessions_opened(bench)
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

/// What the log says `cmd` cost, every execution of it, in log order.
fn durations_of(bench: &Bench, cmd: &str) -> Vec<u64> {
    let mut durations = Vec::new();
    for event in bench.events() {
        let Some(EventPayload::Node(NodeEvent::CriteriaChecked(payload))) = event.payload() else {
            continue;
        };
        for result in &payload.results {
            if result.cmd == cmd {
                durations.extend(result.duration_ms);
            }
        }
    }
    durations
}

/// Asserts that the log priced `cheaper` below `costlier` — every
/// execution of one under every execution of the other. The learned
/// order is only a question worth asking where the answer is this
/// clear; a machine loaded enough to blur these is reported as itself.
fn priced_below(bench: &Bench, cheaper: &str, costlier: &str) {
    let cheap = durations_of(bench, cheaper);
    let costly = durations_of(bench, costlier);
    assert!(
        !cheap.is_empty() && !costly.is_empty(),
        "the log prices both commands: `{cheaper}` {cheap:?}, `{costlier}` {costly:?}"
    );
    assert!(
        cheap.iter().max() < costly.iter().min(),
        "`{cheaper}` {cheap:?} has to cost less than `{costlier}` {costly:?}"
    );
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
    priced_below(&bench, CHEAP_GUARD, MIDDLE_GUARD);
    priced_below(&bench, MIDDLE_GUARD, SECOND_TASK_CRITERION);
    bench.with_cancel(CancellationToken::new())
}

#[tokio::test]
async fn a_resumed_pre_check_runs_criteria_in_the_order_the_log_priced_them() {
    let bench = a_run_whose_log_priced_its_criteria().await;

    // The resume is a separate execution: nothing carries over but the
    // log, which now says what each command cost.
    let RunReport { terminal, state } = bench.wake_on_fixture(FINISHES_THE_SECOND_TASK).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("T002"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    let resumed = pre_checks_of(&bench, "T002");
    assert_eq!(
        resumed.last(),
        Some(&vec![
            CHEAP_GUARD.to_string(),
            MIDDLE_GUARD.to_string(),
            SECOND_TASK_CRITERION.to_string(),
        ]),
        "the resumed pre-check runs cheapest first, off the durations the log holds: {resumed:?}"
    );
}
