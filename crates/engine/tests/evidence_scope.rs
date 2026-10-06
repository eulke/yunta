//! A file the task's own red output points at is evidence that its work
//! reaches there: a write the fence refused, or a path a session asks for
//! citing that criterion, is granted with nobody asked — even in `ask`
//! mode — and the session that was refused picks its work back up.

use yunta_core::events::{Decider, EventPayload, ScopeEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, Bench};

mod common;
use common::*;

/// Red, pointing at `b.rs` the way a compiler does, until `b.rs` exists.
const CHECK: &str = "test -f b.rs && exit 0\necho 'error[E0061]: this function takes 1 argument'\necho '  --> b.rs:3:9'\nexit 1\n";

/// A bench whose repository holds the check task `task-h` is judged by.
fn bench() -> Bench {
    let bench = Bench::new();
    std::fs::write(bench.worktree.join("check.sh"), CHECK).unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "check"]);
    bench
}

/// `task-h` may write `a.rs`; its first session does `first`, and the
/// session picked back up once `b.rs` is granted writes it.
fn fixture(capabilities: &str, first: &str) -> String {
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.rs", "sh check.sh")
    ))
    .replace("capabilities: { run_tools: true }", capabilities);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - {{ path: a.rs, content: \"a\" }}\n{first}    outcome: {{ type: completed, summary: did-h }}\n\
         \x20 - match_prompt_contains: \"was granted\"\n    effects:\n      - {{ path: b.rs, content: \"b\" }}\n    outcome: {{ type: completed, summary: did-b }}\n"
    ));
    fixture
}

/// Who granted `task-h` its scope, if anyone did.
fn granted_by(bench: &Bench) -> Option<Decider> {
    bench
        .events()
        .into_iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Scope(ScopeEvent::Granted(granted))) => {
                Some(granted.decided_by.clone())
            }
            _ => None,
        })
}

#[tokio::test]
async fn refused_write_named_with_a_location_in_red_output_is_granted_and_session_continues() {
    let bench = bench();
    let fixture = fixture(
        "capabilities: { run_tools: true, resume_session: true, fence: tool_calls }",
        "      - { path: b.rs, content: \"b\" }\n",
    );

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", &[], None), &fixture)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    let evidence = Decider::Evidence {
        criterion: "sh check.sh".into(),
    };
    assert_eq!(granted_by(&bench), Some(evidence));
    assert_eq!(
        bench.mock().resumes_seen().len(),
        1,
        "the refused session goes on"
    );
}

#[tokio::test]
async fn a_request_citing_a_red_criterion_is_granted() {
    let bench = bench();
    let request = format!(
        "      - {{ path: {:?}, content: \"paths: [b.rs]\\nreason: the caller breaks\\nevidence: sh check.sh\\n\" }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    );
    let fixture = fixture(
        "capabilities: { run_tools: true, resume_session: true }",
        &request,
    );

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", &[], None), &fixture)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    assert!(matches!(granted_by(&bench), Some(Decider::Evidence { .. })));
}

/// A request that cites nothing is tried against the task's own criteria
/// before anyone is asked: the red one that locates every path asked for
/// grants it, even in `ask` mode.
#[tokio::test]
async fn a_request_citing_nothing_is_granted_on_the_red_criterion_that_locates_it() {
    let bench = bench();
    let request = format!(
        "      - {{ path: {:?}, content: \"paths: [b.rs]\\nreason: the caller breaks\\n\" }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    );
    let fixture = fixture(
        "capabilities: { run_tools: true, resume_session: true }",
        &request,
    );

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", &[], None), &fixture)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-h"), Some(TaskStatus::Done));
    assert_eq!(
        granted_by(&bench),
        Some(Decider::Evidence {
            criterion: "sh check.sh".into()
        })
    );
}

/// The evidence has to locate every path asked for: a request reaching one
/// it does not is a person's to decide, as before.
#[tokio::test]
async fn a_request_citing_nothing_with_a_path_no_red_output_locates_goes_to_a_person() {
    let bench = bench();
    let request = format!(
        "      - {{ path: {:?}, content: \"paths: [b.rs, c.rs]\\nreason: the caller breaks\\n\" }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    );
    let fixture = fixture(
        "capabilities: { run_tools: true, resume_session: true }",
        &request,
    );

    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("ask", &[], None), &fixture)
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(granted_by(&bench), None, "no grant without a person");
    let escalated = bench.events().into_iter().any(|event| {
        matches!(
            event.payload(),
            Some(EventPayload::Scope(ScopeEvent::Requested(_)))
        )
    });
    assert!(escalated, "the request stands, for a person to decide");
}

/// What a run found: a task's change breaks a file outside its scope,
/// which only a guard that holds the task — the project's lint — points
/// at. The session asks for the file with a criterion of its own that
/// already passes, which the loop's rules refuse; the guard's red output,
/// which locates the file, grants it with nobody asked.
#[tokio::test]
async fn a_request_the_rules_refuse_is_granted_on_a_red_guard_that_locates_it() {
    let bench = Bench::new();
    let guard = "if [ -f a.rs ] && [ ! -f b.rs ]; then echo 'error[E0308]: mismatched types'; echo '  --> b.rs:3:9'; exit 1; fi\n";
    tokio::fs::write(bench.worktree.join("guard.sh"), guard)
        .await
        .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "guard"]);
    let tasks = "tasks:\n  - id: task-g\n    title: \"g\"\n    scope: [\"a.rs\"]\n    criteria:\n      - cmd: \"test -f a.rs\"\n      - { cmd: \"sh guard.sh\", type: guard }\n";
    let mut fixture = plan_session(tasks).replace(
        "capabilities: { run_tools: true }",
        "capabilities: { run_tools: true, resume_session: true }",
    );
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-g\"\n    effects:\n      - {{ path: a.rs, content: \"a\" }}\n      - {{ path: {:?}, content: \"paths: [b.rs]\\nreason: its caller breaks\\nproposed_criterion:\\n  cmd: \\\"true\\\"\\n\" }}\n    outcome: {{ type: completed, summary: did-g }}\n\
         \x20 - match_prompt_contains: \"was granted\"\n    effects:\n      - {{ path: b.rs, content: \"b\" }}\n    outcome: {{ type: completed, summary: did-b }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    ));

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", &[], None), &fixture)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("task-g"), Some(TaskStatus::Done));
    assert_eq!(
        granted_by(&bench),
        Some(Decider::Evidence {
            criterion: "sh guard.sh".into()
        })
    );
}
