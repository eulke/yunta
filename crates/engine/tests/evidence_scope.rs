//! A file the task's own red output points at is evidence that its work
//! reaches there: a write the fence refused, or a path a session asks for
//! citing that criterion, is granted with nobody asked — even in `ask`
//! mode — and the session that was refused picks its work back up.

use yunta_core::events::{Decider, EventPayload, GateEvent, ProposedCheck, ScopeEvent, TaskStatus};
use yunta_core::expansion::{Precheck, Waiting};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git, ApproveEverything, Bench, MOCK_CONFIG};

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
fn fixture(first: &str) -> String {
    let mut fixture = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.rs", "sh check.sh")
    ));
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
    let fixture = fixture("      - { path: b.rs, content: \"b\" }\n");

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
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
    let request = request_step(serde_json::json!({
        "paths": ["b.rs"],
        "reason": "the caller breaks",
        "evidence": "sh check.sh",
    }));
    let fixture = fixture(&request);

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
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
    let request = request_step(serde_json::json!({
        "paths": ["b.rs"],
        "reason": "the caller breaks",
    }));
    let fixture = fixture(&request);

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
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
    let request = request_step(serde_json::json!({
        "paths": ["b.rs", "c.rs"],
        "reason": "the caller breaks",
    }));
    let fixture = fixture(&request);

    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
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
    let guard ="if [ -f a.rs ] && [ ! -f b.rs ]; then echo 'error[E0308]: mismatched types'; echo '  --> b.rs:3:9'; exit 1; fi\n";
    tokio::fs::write(bench.worktree.join("guard.sh"), guard)
        .await
        .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "guard"]);
    let tasks = "tasks:\n  - id: task-g\n    title: \"g\"\n    scope: [\"a.rs\"]\n    criteria:\n      - cmd: \"test -f a.rs\"\n      - { cmd: \"sh guard.sh\", type: guard }\n";
    let mut fixture = plan_session(tasks);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-g\"\n    effects:\n      - {{ path: a.rs, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-g }}\n\
         \x20 - match_prompt_contains: \"was granted\"\n    effects:\n      - {{ path: b.rs, content: \"b\" }}\n    outcome: {{ type: completed, summary: did-b }}\n",
        request_step(serde_json::json!({
            "paths": ["b.rs"],
            "reason": "its caller breaks",
            "proposed_criterion": { "cmd": "true" },
        })),
    ));

    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
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

/// A grant the engine makes from a red output records no request the session
/// did not make: the located write is audited as it happened, nothing more.
#[tokio::test]
async fn a_located_grant_records_no_request_the_session_did_not_make() {
    let bench = bench();
    let fixture = fixture("      - { path: b.rs, content: \"b\" }\n");
    bench
        .run(&scope_expansion_workflow("ask", None), &fixture)
        .await;
    assert_eq!(
        granted_by(&bench),
        Some(Decider::Evidence {
            criterion: "sh check.sh".into()
        })
    );
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Scope(ScopeEvent::Requested(_)))
        )),
        "{:?}",
        bench.events()
    );
}

/// A request reaching a protected path is refused whole, even when red output
/// locates the rest of it: evidence never grants a protected path.
#[tokio::test]
async fn a_request_reaching_a_protected_path_is_refused_whole_under_red_output() {
    let bench = Bench::new();
    std::fs::write(
        bench.worktree.join("check.sh"),
        "echo ' --> src/a.rs:1'\necho ' --> .github/ci.yml:2'\nexit 1\n",
    )
    .unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(&bench.worktree, &["commit", "-q", "-m", "check"]);
    let request = request_step(serde_json::json!({
        "paths": ["src/a.rs", ".github/ci.yml"],
        "reason": "x",
        "evidence": "sh check.sh",
    }));
    let config = format!("{MOCK_CONFIG}permissions:\n  paths:\n    deny: [\".github/**\"]\n");
    bench
        .run_with_config(
            &scope_expansion_workflow("ask", None),
            &fixture(&request),
            &config,
        )
        .await;
    assert!(
        bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Scope(ScopeEvent::Denied(_)))
        )),
        "{:?}",
        bench.events()
    );
    assert_eq!(granted_by(&bench), None, "{:?}", bench.events());
}

/// `rules` grants the paths a red criterion locates, but never an instruction
/// file: a person decides about those.
#[tokio::test]
async fn a_rules_grant_never_takes_an_instruction_file() {
    let bench = bench();
    let request = request_step(serde_json::json!({
        "paths": ["AGENTS.md"],
        "reason": "house rules",
        "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
    }));
    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("rules", None), &fixture(&request))
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(granted_by(&bench), None, "{:?}", bench.events());
}

/// `ask` never grants an instruction file on its own: the run waits for a
/// person.
#[tokio::test]
async fn an_instruction_file_request_under_ask_waits_for_a_person() {
    let bench = bench();
    let request = request_step(serde_json::json!({
        "paths": ["AGENTS.md"],
        "reason": "house rules",
    }));
    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("ask", None), &fixture(&request))
        .await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(granted_by(&bench), None, "{:?}", bench.events());
}

/// A request that reaches an agent's own configuration is refused whole, even
/// when the rest of it is in scope and its criterion is red.
#[tokio::test]
async fn a_request_reaching_an_agent_config_file_is_refused_whole() {
    let bench = bench();
    let request = request_step(serde_json::json!({
        "paths": ["src/a.rs", ".claude/settings.json"],
        "reason": "x",
        "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
    }));
    bench
        .run(&scope_expansion_workflow("rules", None), &fixture(&request))
        .await;
    assert_eq!(granted_by(&bench), None, "{:?}", bench.events());
    assert!(
        bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Scope(ScopeEvent::Denied(_)))
        )),
        "{:?}",
        bench.events()
    );
}

/// A grant a person makes for an instruction file lands on disk, and the close
/// does not take the file for a trespass.
#[tokio::test]
async fn a_person_granted_instruction_file_lands_without_a_trespass() {
    let bench = bench();
    // The criterion is green once the granted instruction file exists, so the
    // task's check passes on the file the person granted and nothing else.
    std::fs::write(bench.worktree.join("check.sh"), "test -f AGENTS.md\n").unwrap();
    git(&bench.worktree, &["add", "-A"]);
    git(
        &bench.worktree,
        &["commit", "-q", "-m", "check the instruction file"],
    );
    let request = request_step(serde_json::json!({
        "paths": ["AGENTS.md"],
        "reason": "house rules",
    }));
    let fixture = fixture(&request).replace(
        "path: b.rs, content: \"b\"",
        "path: AGENTS.md, content: \"rules\"",
    );
    // The task's first menu option is its grant; the test does not name its id.
    let human = ApproveEverything::new("lead");
    bench
        .run_with_interaction(&scope_expansion_workflow("ask", None), &fixture, &human)
        .await;
    assert!(
        matches!(granted_by(&bench), Some(Decider::Person { .. })),
        "{:?}",
        bench.events()
    );
    assert!(
        bench.worktree.join("AGENTS.md").exists(),
        "{:?}",
        bench.events()
    );
}

/// A request the loop cannot decide is put to a person with the typed reason
/// the loop waits on, and with the criterion the session proposed and what the
/// engine's own run of it found.
#[tokio::test]
async fn a_gate_for_an_undecided_request_records_why_the_loop_waits() {
    let bench = bench();
    let request = request_step(serde_json::json!({
        "paths": ["b.rs", "c.rs"],
        "reason": "the caller breaks",
        "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
    }));
    let human = ApproveEverything::new("lead");
    bench
        .run_with_interaction(
            &scope_expansion_workflow("ask", None),
            &fixture(&request),
            &human,
        )
        .await;
    let waiting = bench
        .events()
        .into_iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(payload))) => Some(payload.clone()),
            _ => None,
        })
        .expect("the gate a person answered");
    // The record carries the paths alone as facts: why the loop waits is the
    // typed value above, and no sentence about it is stored in the evidence.
    let facts = waiting.evidence().facts();
    let labels: Vec<Option<&str>> = facts.iter().map(|fact| fact.label.as_deref()).collect();
    assert_eq!(labels, vec![Some("paths")]);
    let scope = waiting.scope().expect("the gate says why the loop waits");
    assert_eq!(scope.because, Waiting::ModeAsk);
    assert_eq!(
        scope.proposed,
        Some(ProposedCheck {
            cmd: "test -f nonexistent-marker".into(),
            precheck: Precheck::Red { exit: 1 },
        })
    );
}
