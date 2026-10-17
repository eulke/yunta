//! What a task session reads from the run: the
//! plan its task belongs to, its own task and every cycle it ran, and which
//! guard is the run's suite.

use super::*;

/// A `criteria_checked` of T001, with each command's exit code.
fn checked(phase: Phase, exits: [i32; 2]) -> EventPayload {
    EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
        task_id: TaskId::from("T001"),
        phase,
        results: vec![
            CriterionResult {
                cmd: "test -f hello.txt".to_string(),
                exit_code: exits[0],
                r#type: None,
                reused: false,
                duration_ms: None,
                output: None,
                tail: Vec::new(),
                tree: None,
                head: None,
                conclusion: None,
            },
            CriterionResult {
                cmd: "true".to_string(),
                exit_code: exits[1],
                r#type: Some(CriterionType::Guard),
                reused: false,
                duration_ms: None,
                output: None,
                tail: Vec::new(),
                tree: None,
                head: None,
                conclusion: None,
            },
        ],
        waiting: Vec::new(),
    }))
}

/// The same check, with the last lines its red criterion printed.
fn saying(check: EventPayload, tail: &[&str]) -> EventPayload {
    let EventPayload::Node(NodeEvent::CriteriaChecked(mut check)) = check else {
        unreachable!("`checked` builds a criteria check")
    };
    check.results[0].tail = tail.iter().map(|line| line.to_string()).collect();
    EventPayload::Node(NodeEvent::CriteriaChecked(check))
}

/// A log in the middle of T001's second cycle: a first cycle whose one
/// attempt broke the guard, then the loop setting it running again, this
/// cycle's pre-check, and one attempt that left the criterion red and
/// wrote outside scope.
fn two_cycles_of_one_attempt_each(host: &ToolsHost) {
    let registered = host.record(
        Some("plan"),
        EventPayload::Tasks(TaskEvent::Registered(
            yunta_core::events::TaskRegisteredPayload {
                task_id: TaskId::from("T001"),
                criteria: Vec::new(),
                scope: Vec::new(),
                depends_on: Vec::new(),
            },
        )),
    );
    let running = || {
        EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::to(
            TaskId::from("T001"),
            TaskStatus::Running,
            registered,
        )))
    };
    // A first cycle whose one attempt broke the guard.
    host.record(Some("implement"), running());
    host.record(Some("implement"), checked(Phase::Post, [1, 1]));
    host.record(Some("implement"), running());
    host.record(Some("implement"), checked(Phase::Pre, [1, 0]));
    host.record(
        Some("implement"),
        saying(checked(Phase::Post, [1, 0]), &["hello.txt is missing"]),
    );
    host.record(
        Some("implement"),
        EventPayload::Node(NodeEvent::ScopeChecked(ScopeCheckedPayload {
            task_id: Some(TaskId::from("T001")),
            diff: vec!["notes.md".into()],
            violations: vec!["notes.md".into()],
            denied: Vec::new(),
        })),
    );
}

/// A plan whose design declares a type one task creates and another
/// uses, and whose tasks split the files between them.
fn colored_plan() -> yunta_core::TasksFile {
    yunta_core::yaml::parse(
        r#"
summary: Color the CLI's messages
description: One palette, used by every surface.
design: |
  ```rust
  enum ColorRole { Error, Warning, Success, Info }
  ```
shapes:
  - { name: ColorRole, owner: T002, file: src/color.rs, code: "enum ColorRole { Error, Warning, Success, Info }" }
risks: [Low contrast on some themes]
out_of_scope: [Truecolor]
tasks:
  - id: T001
    title: Write the greeting
    scope: [hello.txt]
    criteria: [{ cmd: test -f hello.txt }]
  - id: T002
    title: Add the palette
    scope: [src/color.rs]
    criteria: [{ cmd: test -f src/color.rs }]
    depends_on: [T001]
"#,
    )
    .unwrap()
}

#[tokio::test]
async fn a_task_session_reads_the_plan_its_task_belongs_to() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let mut access = host.task_access(greeting_task(), unit_at(host.attempt_dir()));
    access.plan = Some(Arc::new(colored_plan()));
    let session = host.task_session("implement", access).await;
    let client = client_for(&session, None).await.unwrap();

    let (is_error, text) = call(&client, "yunta_task", json!({})).await;

    assert!(!is_error, "got: {text}");
    let sheet: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        sheet["plan"],
        json!({
            "summary": "Color the CLI's messages",
            "description": "One palette, used by every surface.",
            "design": "```rust\nenum ColorRole { Error, Warning, Success, Info }\n```",
            "shapes": [
                {"name": "ColorRole", "owner": "T002", "file": "src/color.rs", "code": "enum ColorRole { Error, Warning, Success, Info }"},
            ],
            "risks": ["Low contrast on some themes"],
            "out_of_scope": ["Truecolor"],
            "other_tasks": [
                {"id": "T002", "title": "Add the palette", "scope": ["src/color.rs"], "depends_on": ["T001"]},
            ],
        }),
        "the design the task names, and who owns what its scope leaves out"
    );
}

#[tokio::test]
async fn a_task_session_reads_its_task_and_every_cycle_it_ran_from_the_run() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    two_cycles_of_one_attempt_each(&host);

    let mut access = host.task_access(greeting_task(), unit_at(host.attempt_dir()));
    access.scope.push("docs/**".into());
    let session = host.task_session("implement", access).await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_task", json!({})).await;
    assert!(!is_error, "got: {text}");
    let red = json!({"cmd": "test -f hello.txt", "guard": false, "exit_code": 1});
    let red_saying_why = json!({
        "cmd": "test -f hello.txt", "guard": false, "exit_code": 1,
        "tail": ["hello.txt is missing"],
    });
    let guard = json!({"cmd": "true", "guard": true, "exit_code": 0});
    let broken_guard = json!({"cmd": "true", "guard": true, "exit_code": 1});
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        json!({
            "id": "T001",
            "title": "Write the greeting",
            "notes": "the greeting lives in hello.txt",
            "description": "Greets whoever opens the project.",
            "scope": ["hello.txt", "docs/**"],
            "criteria": [
                {"cmd": "test -f hello.txt", "guard": false},
                {"cmd": "true", "guard": true},
            ],
            "cycles": [
                {"cycle": 1, "checks": [
                    {"phase": "post", "attempt": 1, "criteria": [red, broken_guard]},
                ]},
                {"cycle": 2, "checks": [
                    {"phase": "pre", "criteria": [red, guard]},
                    {"phase": "post", "attempt": 1, "criteria": [red_saying_why, guard], "outside_scope": ["notes.md"]},
                ]},
            ],
        }),
        "the task the cycle judges by, its granted scope included, and every cycle it ran"
    );
    client.cancel().await.unwrap();
}

/// The suite the run holds every task to is one of the task's guards,
/// and says what it is there to show, so the session knows why a change
/// that breaks it keeps the task open.
#[tokio::test]
async fn a_task_session_reads_which_guard_is_the_runs_suite() {
    let host = ToolsHost::over(BLACKBOARD_WORKFLOW);
    let task = yunta_engine::judged_task(&greeting_task(), &["make test".to_string()]);
    let session = host
        .task_session(
            "implement",
            host.task_access(task, unit_at(host.attempt_dir())),
        )
        .await;
    let client = client_for(&session, None).await.unwrap();
    let (is_error, text) = call(&client, "yunta_task", json!({})).await;
    assert!(!is_error, "got: {text}");
    let sheet: serde_json::Value = serde_json::from_str(&text).unwrap();
    let criteria = sheet["criteria"].as_array().expect("the task's criteria");
    assert_eq!(
        criteria[..2],
        [
            json!({"cmd": "test -f hello.txt", "guard": false}),
            json!({"cmd": "true", "guard": true}),
        ],
        "what the document declares, as it declares it"
    );
    assert_eq!(criteria[2]["cmd"], "make test");
    assert_eq!(criteria[2]["guard"], true);
    assert!(criteria[2]["proves"].is_string(), "{sheet:#}");
    client.cancel().await.unwrap();
}
