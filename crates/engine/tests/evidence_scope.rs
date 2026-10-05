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
