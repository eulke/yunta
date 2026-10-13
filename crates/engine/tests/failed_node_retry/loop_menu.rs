//! The menu of a loop whose tasks stopped: what a blocked task left, what it
//! offers, and the stop it records as data.

use super::*;

/// A loop whose one task needs `made.txt`: the plan writes the task, and
/// only a session that creates the file meets its criterion.
const ONE_TASK_LOOP_WORKFLOW: &str = r#"
name: stuck-task
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Make it\n    scope: [made.txt]\n    criteria:\n      - cmd: test -f made.txt\n' > {{node.artifacts}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

/// An attempt that never writes the file: the task blocks after it.
const MISSES: &str = "\
dialect: claude-code
sessions:
  - outcome: { type: completed, summary: missed }
";

/// One attempt that writes it.
const MAKES_IT: &str = "\
dialect: claude-code
sessions:
  - effects:
      - { path: made.txt, content: made }
    outcome: { type: completed, summary: made }
";

fn status_of(bench: &Bench, task: &str) -> Option<yunta_core::events::TaskStatus> {
    yunta_engine::derive(&bench.events()).tasks.status(task)
}

#[tokio::test]
async fn a_blocked_task_says_what_its_attempt_left_red() {
    let bench = parked(ONE_TASK_LOOP_WORKFLOW, MISSES).await;

    assert_eq!(
        status_of(&bench, "T001"),
        Some(yunta_core::events::TaskStatus::Blocked)
    );
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("a loop that failed on a blocked task is a decision");
    let menu = escalation
        .failure
        .as_ref()
        .expect("the menu carries the failure it answers");
    let Failure::TasksStopped { stopped } = &menu.failure else {
        panic!("the decision names the stopped task: {:?}", menu.failure);
    };
    let [TaskStop {
        task_id,
        cause: TaskCause::Unmet {
            attempts: 1, red, ..
        },
    }] = stopped.as_slice()
    else {
        panic!("one task, not done after its one attempt: {stopped:?}");
    };
    assert_eq!(task_id.as_str(), "T001");
    assert!(
        red.iter()
            .any(|run| run.cmd == "test -f made.txt" && run.exit_code == 1),
        "the decision names the criterion still red: {red:?}"
    );
}

/// A blocked loop offers no retry on the tree it failed on: its criterion
/// answers the same on that tree, so answering a retry the menu does not
/// offer is refused, and the task stays blocked.
#[tokio::test]
async fn a_blocked_loop_offers_no_retry_on_the_tree_it_failed_on() {
    let bench = parked(ONE_TASK_LOOP_WORKFLOW, MISSES).await;

    assert_eq!(options(&bench).await, ["abort"]);
    assert!(
        matches!(
            answer_parked(&bench, "retry").await,
            Err(yunta_engine::ResolveGateError::UnknownOption { .. })
        ),
        "no retry is offered on the tree the loop blocked on"
    );
    assert_eq!(status_of(&bench, "T001"), Some(TaskStatus::Blocked));
}

#[tokio::test]
async fn a_plain_resume_leaves_a_blocked_task_blocked() {
    let bench = parked(ONE_TASK_LOOP_WORKFLOW, MISSES).await;

    let RunReport { terminal, .. } = bench.wake_on_fixture(MAKES_IT).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(
        status_of(&bench, "T001"),
        Some(yunta_core::events::TaskStatus::Blocked)
    );
    assert!(
        bench.mock().requests_seen().is_empty(),
        "nobody chose to spend again, so no session opened"
    );
}

/// A loop whose one task writes `made.txt`, and whose criterion also
/// needs `marker` — a file outside the repository that only a person
/// provides. The sessions do the task's work; the criterion stays red
/// until the person does their part.
fn needs_a_person(marker: &std::path::Path) -> String {
    format!(
        r#"
name: needs-a-person
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Make it\n    scope: [made.txt]\n    criteria:\n      - cmd: test -f made.txt && test -f {marker}\n' > {{{{node.artifacts}}}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#,
        marker = marker.display()
    )
}

/// Three attempts: the first writes the task's own work, none can meet
/// what the person has to provide.
const DOES_ITS_PART: &str = "\
dialect: claude-code
sessions:
  - effects:
      - { path: made.txt, content: made }
    outcome: { type: completed, summary: made }
  - outcome: { type: completed, summary: nothing more to do }
  - outcome: { type: completed, summary: nothing more to do }
";

async fn options(bench: &Bench) -> Vec<String> {
    let (_, escalation) = current_escalation(
        &bench.manifest(),
        &bench.run_dir(),
        &yunta_engine::derive(&bench.events()),
    )
    .await
    .unwrap()
    .expect("a failed loop is a decision");
    escalation
        .options()
        .iter()
        .map(|option| option.id.to_string())
        .collect()
}

#[tokio::test]
async fn continuing_a_loop_from_the_work_it_left_closes_the_task_without_a_session() {
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("provided");
    let bench = parked(&needs_a_person(&marker), DOES_ITS_PART).await;
    assert_eq!(
        options(&bench).await,
        ["continue-work", "abort"],
        "the blocked task left work, so continuing from it is offered; its tree has not moved, so a retry is not"
    );

    write(&marker, "done");
    answer_parked(&bench, "continue-work").await.unwrap();
    let RunReport { terminal, state } = bench
        .wake_on_fixture("dialect: claude-code\nsessions: []\n")
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");
    assert_eq!(
        state.tasks.status("T001"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert!(
        bench.mock().requests_seen().is_empty(),
        "the work it left closed the task, so no session was spent on it"
    );
}

#[tokio::test]
async fn a_task_blocked_before_any_work_is_only_offered_to_run_again() {
    let bench = parked(
        &ONE_TASK_LOOP_WORKFLOW.replace("test -f made.txt", "yunta-no-such-tool"),
        MISSES,
    )
    .await;

    assert_eq!(
        options(&bench).await,
        ["retry", "abort"],
        "the pre-check blocked it before any work, so there is nothing to continue from"
    );
}

#[tokio::test]
async fn a_loop_failure_keeps_its_stopped_tasks_as_data() {
    let bench = parked(
        &ONE_TASK_LOOP_WORKFLOW.replace("test -f made.txt", "yunta-no-such-tool"),
        MISSES,
    )
    .await;
    let failure = bench.events().iter().find_map(|e| match e.payload() {
        Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.failure.clone()),
        _ => None,
    });
    let Some(failure) = failure else {
        panic!("the loop does not fail")
    };
    assert!(
        matches!(failure, Failure::TasksStopped { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_blocked_task_records_its_stop_on_its_status_event() {
    let bench = parked(
        &ONE_TASK_LOOP_WORKFLOW.replace("test -f made.txt", "yunta-no-such-tool"),
        MISSES,
    )
    .await;
    let stops = bench
        .events()
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(EventPayload::Tasks(TaskEvent::StatusChanged(p)))
                    if p.new_status == TaskStatus::Blocked && p.stop.is_some()
            )
        })
        .count();
    assert_eq!(stops, 1, "the blocked task carries its stop");
}

/// A loop whose one task's criterion runs `tool`, a command that does not
/// exist until a person puts it there. It lives outside the repository, so
/// the tree the task left stays as it was.
fn needs_a_tool(tool: &std::path::Path) -> String {
    format!(
        r#"
name: needs-a-tool
nodes:
  - id: plan
    kind: bash
    run: "printf 'tasks:\n  - id: T001\n    title: Make it\n    scope: [made.txt]\n    criteria:\n      - cmd: {tool}\n' > {{{{node.artifacts}}}}/tasks.yaml"
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#,
        tool = tool.display()
    )
}

/// Writes `tool` as a shell script that passes once `made.txt` exists, and
/// makes it executable, as the criterion runs it.
fn make_runnable(tool: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    write(tool, "#!/bin/sh\ntest -f made.txt\n");
    // 0o755: the owner reads, writes and runs it, and everyone else reads and runs it.
    std::fs::set_permissions(tool, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The reopening of T001 cites the latest resolved decision: its status change
/// to pending carries that decision's seq as the cause.
fn assert_reopened_by_the_last_resolution(bench: &Bench) {
    let events = bench.events();
    let decision = events
        .iter()
        .rev()
        .find(|e| {
            matches!(
                e.payload(),
                Some(EventPayload::Gates(GateEvent::Resolved(_)))
            )
        })
        .map(|e| e.seq)
        .expect("the retry is on the log");
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(EventPayload::Tasks(TaskEvent::StatusChanged(p)))
            if p.task_id.as_str() == "T001"
                && p.new_status == TaskStatus::Pending
                && p.caused_by == decision
    )));
}

/// A blocked task is offered a retry once a person changes the tree the loop
/// stopped on: the wake after the change records the tree, and the retry
/// reopens the task with a fresh cycle that cites the decision that asked for it.
#[tokio::test]
async fn retrying_a_loop_gives_the_task_it_left_blocked_a_fresh_cycle() {
    let bench = parked(ONE_TASK_LOOP_WORKFLOW, MISSES).await;
    assert_eq!(options(&bench).await, ["abort"]);

    // The person changes the tree; the retry is offered only after the wake records it.
    write(&bench.worktree.join("notes.txt"), "a note the person left");
    let RunReport { terminal, .. } = bench.wake().await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(options(&bench).await, ["retry", "abort"]);

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, state } = bench.wake_on_fixture(MAKES_IT).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(status_of(&bench, "T001"), Some(TaskStatus::Done));
    assert_reopened_by_the_last_resolution(&bench);
    assert_eq!(state.tasks.status("T001"), Some(TaskStatus::Done));
}

/// A blocked task whose criterion could not run is offered a retry once its
/// command can run, and the retry reopens it with a fresh cycle that cites the
/// decision that asked for it.
#[tokio::test]
async fn retrying_a_loop_gives_the_task_it_left_blocked_a_fresh_cycle_once_its_command_can_run() {
    let outside = tempfile::tempdir().unwrap();
    let tool = outside.path().join("check");
    let bench = parked(&needs_a_tool(&tool), MISSES).await;
    assert_eq!(options(&bench).await, ["retry", "abort"]);

    // The person makes the command runnable. It passes once the work is done.
    make_runnable(&tool);
    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, state } = bench.wake_on_fixture(MAKES_IT).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(status_of(&bench, "T001"), Some(TaskStatus::Done));
    assert_reopened_by_the_last_resolution(&bench);
    assert_eq!(state.tasks.status("T001"), Some(TaskStatus::Done));
}
