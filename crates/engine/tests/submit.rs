//! An interpreted artifact, end to end: a mock session acting as a real
//! MCP client hands a document over, the engine validates it while the
//! session can still act, and writes the file its close reads back.
//!
//! What every test here is really about is the seam between writing and
//! judging. It used to be the session's own end — a document was judged
//! after the session that wrote it had died, so every mistake cost a
//! whole session. The tools move it inside: a refusal comes back as an
//! answer, and the session fixes it in the same breath.

use yunta_core::events::{EventPayload, SubmissionOutcome};
use yunta_engine::{NodeState, RunTerminal};
use yunta_testkit::Bench;

const PLAN_NODE: &str = r#"
name: plan
nodes:
  - id: plan
    kind: prompt
    runner: executor
    prompt: "Write a task ledger."
    artifacts:
      produces: [{ name: plan.yaml, kind: task-ledger }]
"#;

const REVIEW_NODE: &str = r#"
name: review
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the change."
    artifacts:
      produces: [{ name: review.yaml, kind: findings }]
"#;

/// A `run_tool` step's arguments, as a fixture writes them.
fn ledger_document(tasks: &[(&str, &str)]) -> String {
    let entries: String = tasks
        .iter()
        .map(|(id, title)| {
            format!(
                "              - id: {id}\n                title: \"{title}\"\n\
                 \x20               scope: [\"src/{id}/**\"]\n\
                 \x20               criteria:\n                  - cmd: \"cargo test {id}\"\n"
            )
        })
        .collect();
    format!("            tasks:\n{entries}")
}

fn submitted(bench: &Bench) -> Vec<(String, bool)> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::ArtifactSubmitted(p)) => Some((
                p.name.clone(),
                matches!(p.outcome, SubmissionOutcome::Accepted { .. }),
            )),
            _ => None,
        })
        .collect()
}

fn refusals(bench: &Bench) -> Vec<yunta_core::diagnostic::Report> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::ArtifactSubmitted(p)) => match &p.outcome {
                SubmissionOutcome::Refused { report } => Some(report.clone()),
                SubmissionOutcome::Accepted { .. } => None,
            },
            _ => None,
        })
        .collect()
}

fn kinds(bench: &Bench, wanted: &str) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| event.body.kind_name() == wanted)
        .count()
}

#[tokio::test]
async fn a_session_submits_a_ledger_and_the_engine_writes_the_file() {
    let bench = Bench::new();
    let fixture = format!(
        r#"
capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        arguments:
          name: plan.yaml
          document:
{}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        ledger_document(&[("alpha", "First"), ("beta", "Second")])
    );

    let (terminal, state) = bench.run(PLAN_NODE, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    assert_eq!(submitted(&bench), vec![("plan.yaml".to_string(), true)]);
    assert_eq!(kinds(&bench, "artifact_written"), 1);
    assert_eq!(kinds(&bench, "task_registered"), 2);
    assert_eq!(kinds(&bench, "node_failed"), 0);

    // The file is the engine's, and it reads back as the document.
    let bytes = bench.artifact("plan.yaml").expect("the engine wrote it");
    let ledger: yunta_core::Ledger =
        yunta_core::shape::read(&bytes, "plan.yaml").expect("a canonical ledger");
    let ids: Vec<String> = ledger.tasks.iter().map(|t| t.id.to_string()).collect();
    assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
}

#[tokio::test]
async fn a_node_that_only_submits_is_never_granted_the_artifact_directory() {
    let bench = Bench::new();
    let fixture = format!(
        r#"
capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        arguments:
          name: plan.yaml
          document:
{}
    outcome: {{ type: completed, summary: "planned" }}
"#,
        ledger_document(&[("alpha", "First")])
    );
    bench.run(PLAN_NODE, &fixture).await;
    assert_eq!(
        bench.mock().artifact_dirs_seen(),
        vec![None],
        "an interpreted artifact is the engine's to write, so the session \
         needs no reach outside its worktree"
    );
}

#[tokio::test]
async fn a_refused_submission_names_every_rule_problem_in_the_same_session() {
    let bench = Bench::new();
    // A dependency on a task nobody declared, and a cycle: two rules of
    // the same document, both reported at once.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        expect: refused
        arguments:
          name: plan.yaml
          document:
            tasks:
              - id: alpha
                title: "First"
                scope: ["src/alpha/**"]
                depends_on: [beta, ghost]
                criteria:
                  - cmd: "cargo test alpha"
              - id: beta
                title: "Second"
                scope: ["src/beta/**"]
                depends_on: [alpha]
                criteria:
                  - cmd: "cargo test beta"
      - type: run_tool
        tool: yunta_submit_task_ledger
        arguments:
          name: plan.yaml
          document:
            tasks:
              - id: alpha
                title: "First"
                scope: ["src/alpha/**"]
                criteria:
                  - cmd: "cargo test alpha"
    outcome: { type: completed, summary: "planned after a correction" }
"#;

    let (terminal, state) = bench.run(PLAN_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");

    // One attempt: a refusal is an answer, not the end of the session.
    assert_eq!(kinds(&bench, "node_started"), 1);
    assert_eq!(
        submitted(&bench),
        vec![
            ("plan.yaml".to_string(), false),
            ("plan.yaml".to_string(), true)
        ]
    );

    let report = refusals(&bench).pop().expect("the refusal is on the log");
    let codes: Vec<&str> = report
        .diagnostics
        .iter()
        .map(|d| d.problem.code())
        .collect();
    assert!(
        codes.contains(&"unknown-dependency") && codes.contains(&"dependency-cycle"),
        "every rule at once: {codes:?}"
    );
}

#[tokio::test]
async fn a_structural_problem_is_reported_with_its_path() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        expect: refused
        arguments:
          name: plan.yaml
          document:
            tasks:
              - id: alpha
                title: "First"
                scope: ["src/alpha/**"]
                manual_review: "yes"
                criteria:
                  - cmd: "cargo test alpha"
    outcome: { type: completed, summary: "gave up" }
"#;
    bench.run(PLAN_NODE, fixture).await;

    let report = refusals(&bench).pop().expect("the refusal is on the log");
    let rendered = report.diagnostics[0].to_string();
    assert!(
        rendered.contains("tasks[0].manual_review"),
        "the path locates the value: {rendered}"
    );
}

#[tokio::test]
async fn the_same_document_submitted_twice_yields_the_same_hash() {
    let bench = Bench::new();
    // The second submission writes the same keys in another order: what
    // the engine holds is the same document, so the file is the same
    // bytes and the same hash.
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        arguments:
          name: plan.yaml
          document:
            tasks:
              - id: alpha
                title: "First"
                scope: ["src/alpha/**"]
                criteria:
                  - cmd: "cargo test alpha"
      - type: run_tool
        tool: yunta_submit_task_ledger
        arguments:
          name: plan.yaml
          document:
            tasks:
              - criteria:
                  - cmd: "cargo test alpha"
                scope: ["src/alpha/**"]
                title: "First"
                id: alpha
    outcome: { type: completed, summary: "planned" }
"#;
    bench.run(PLAN_NODE, fixture).await;

    let hashes: Vec<String> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::ArtifactSubmitted(p)) => match &p.outcome {
                SubmissionOutcome::Accepted { content_hash } => Some(content_hash.to_string()),
                SubmissionOutcome::Refused { .. } => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(hashes.len(), 2);
    assert_eq!(hashes[0], hashes[1], "one meaning, one file, one hash");
}

#[tokio::test]
async fn a_name_the_node_did_not_declare_is_refused() {
    let bench = Bench::new();
    let fixture = format!(
        r#"
capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_task_ledger
        expect: refused
        arguments:
          name: other.yaml
          document:
{}
    outcome: {{ type: completed, summary: "submitted the wrong name" }}
"#,
        ledger_document(&[("alpha", "First")])
    );

    let (terminal, state) = bench.run(PLAN_NODE, &fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "nothing was submitted, so the node fails on a missing artifact: {state:?}"
    );
    assert!(
        submitted(&bench).is_empty(),
        "a name the node never declared is not a submission of anything"
    );
    match state.nodes.get("plan") {
        Some(NodeState::Failed { failure, .. }) => {
            let text = failure.to_string();
            assert!(text.contains("plan.yaml"), "got: {text}");
        }
        other => panic!("expected plan failed, got {other:?}"),
    }
}

#[tokio::test]
async fn a_document_never_submitted_fails_the_node_with_no_second_session() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - outcome: { type: completed, summary: "said it was done" }
"#;
    let (terminal, state) = bench.run(PLAN_NODE, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");

    // One attempt, and no session opened to correct anything: a document
    // nobody handed over is a node that failed, not a node to re-instruct.
    assert_eq!(kinds(&bench, "node_started"), 1);
    assert_eq!(kinds(&bench, "agent_session_opened"), 1);

    let retryable = bench
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::NodeFailed(p)) => Some(p.retryable),
            _ => None,
        })
        .expect("the node failed");
    assert!(!retryable, "there is nothing to attempt again");
}

#[tokio::test]
async fn a_typed_artifact_on_an_adapter_without_run_tools_fails_before_dispatch() {
    let bench = Bench::new();
    // The capability is absent, so the document has no way in. The node
    // fails before a session opens rather than after one produced
    // nothing.
    let fixture = r#"
sessions:
  - outcome: { type: completed, summary: "never reached" }
"#;
    let (terminal, state) = bench.run(PLAN_NODE, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");

    match state.nodes.get("plan") {
        Some(NodeState::Failed { failure, .. }) => {
            let text = failure.to_string();
            assert!(
                text.contains("run_tools") && text.contains("plan.yaml"),
                "the refusal names the capability and the artifact: {text}"
            );
        }
        other => panic!("expected plan failed, got {other:?}"),
    }
    assert_eq!(kinds(&bench, "agent_session_opened"), 0);
    assert_eq!(kinds(&bench, "capability_degraded"), 0);
}

#[tokio::test]
async fn a_review_reports_findings_and_the_engine_derives_the_file() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: null-deref
          severity: blocking
          title: "Resize handler dereferences a null pointer"
          location: "src/ui/resize.rs:142"
          detail: "Resizing before the first paint reaches a null surface."
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: stale-doc
          severity: minor
          title: "The README describes the old flag"
          location: "README.md:14"
          detail: "The flag it names was renamed two releases ago."
    outcome: { type: completed, summary: "reviewed" }
"#;
    let (terminal, state) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    let ids: Vec<String> = file.findings.iter().map(|f| f.id.to_string()).collect();
    assert_eq!(
        ids,
        vec!["null-deref".to_string(), "stale-doc".to_string()],
        "in the order they were reported"
    );

    // Reported once, on the log once: the file is what those reports add
    // up to, never a second telling of them.
    assert_eq!(kinds(&bench, "finding_posted"), 2);
    assert_eq!(kinds(&bench, "artifact_written"), 1);
}

#[tokio::test]
async fn a_review_that_reports_nothing_closes_with_an_empty_findings_file() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - outcome: { type: completed, summary: "found nothing" }
"#;
    let (terminal, state) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished, "{state:?}");

    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    assert!(file.findings.is_empty(), "a review that found nothing");
    assert_eq!(kinds(&bench, "finding_posted"), 0);
    assert_eq!(kinds(&bench, "artifact_written"), 1);
}

#[tokio::test]
async fn a_finding_with_an_unknown_key_is_refused_and_nothing_is_recorded() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        expect: refused
        arguments:
          id: null-deref
          severity: blocking
          title: "t"
          location: "src/a.rs:1"
          detail: "d"
          line: 42
    outcome: { type: completed, summary: "reviewed" }
"#;
    bench.run(REVIEW_NODE, fixture).await;

    assert_eq!(kinds(&bench, "finding_posted"), 0);
    assert_eq!(kinds(&bench, "finding_refused"), 1);
    let report = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::FindingRefused(p)) => Some(p.report.clone()),
            _ => None,
        })
        .expect("the refusal is on the log");
    let rendered = report.diagnostics[0].to_string();
    assert!(
        rendered.contains("line"),
        "the refusal names the key: {rendered}"
    );
}

#[tokio::test]
async fn a_finding_id_already_posted_by_the_node_is_refused() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: null-deref
          severity: blocking
          title: "first telling"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_post_finding
        expect: refused
        arguments:
          id: null-deref
          severity: minor
          title: "second telling"
          location: "src/b.rs:2"
          detail: "d"
    outcome: { type: completed, summary: "reviewed" }
"#;
    let (terminal, _) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(kinds(&bench, "finding_posted"), 1);
    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    assert_eq!(file.findings.len(), 1);
    assert_eq!(file.findings[0].title, "first telling");
}

#[tokio::test]
async fn an_updated_finding_replaces_its_state_and_keeps_its_place() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: null-deref
          severity: minor
          title: "looked minor"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: stale-doc
          severity: note
          title: "second"
          location: "README.md:1"
          detail: "d"
      - type: run_tool
        tool: yunta_update_finding
        arguments:
          id: null-deref
          severity: blocking
          title: "it is reached on every run"
          location: "src/a.rs:1-14"
          detail: "d"
    outcome: { type: completed, summary: "reviewed" }
"#;
    let (terminal, _) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    let rendered: Vec<(String, String)> = file
        .findings
        .iter()
        .map(|f| (f.id.to_string(), f.title.clone()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (
                "null-deref".to_string(),
                "it is reached on every run".to_string()
            ),
            ("stale-doc".to_string(), "second".to_string()),
        ],
        "an update replaces the content and moves nothing"
    );
    assert_eq!(kinds(&bench, "finding_updated"), 1);
}

#[tokio::test]
async fn a_withdrawn_finding_leaves_the_file_but_not_the_log() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: false-alarm
          severity: major
          title: "looked wrong"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: real-one
          severity: major
          title: "this one stands"
          location: "src/b.rs:2"
          detail: "d"
      - type: run_tool
        tool: yunta_withdraw_finding
        arguments:
          id: false-alarm
          reason: "the call it named is gone"
    outcome: { type: completed, summary: "reviewed" }
"#;
    let (terminal, _) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    let ids: Vec<String> = file.findings.iter().map(|f| f.id.to_string()).collect();
    assert_eq!(ids, vec!["real-one".to_string()]);

    // The log keeps it, and the reason.
    assert_eq!(kinds(&bench, "finding_posted"), 2);
    let reason = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::FindingWithdrawn(p)) => Some(p.reason.clone()),
            _ => None,
        })
        .expect("the withdrawal is on the log");
    assert_eq!(reason, "the call it named is gone");
}

#[tokio::test]
async fn a_withdrawn_id_is_final() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: gone
          severity: major
          title: "t"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_withdraw_finding
        arguments:
          id: gone
          reason: "a false positive"
      - type: run_tool
        tool: yunta_post_finding
        expect: refused
        arguments:
          id: gone
          severity: major
          title: "back again"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_update_finding
        expect: refused
        arguments:
          id: gone
          severity: minor
          title: "or edited"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_withdraw_finding
        expect: refused
        arguments:
          id: gone
          reason: "again"
    outcome: { type: completed, summary: "reviewed" }
"#;
    let (terminal, _) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    assert_eq!(kinds(&bench, "finding_refused"), 3);
    let operations: Vec<String> = bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::FindingRefused(p)) => Some(format!("{:?}", p.operation)),
            _ => None,
        })
        .collect();
    assert_eq!(operations, vec!["Post", "Update", "Withdraw"]);

    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    assert!(file.findings.is_empty());
}

#[tokio::test]
async fn a_withdrawal_without_a_reason_is_refused() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: real-one
          severity: major
          title: "t"
          location: "src/a.rs:1"
          detail: "d"
      - type: run_tool
        tool: yunta_withdraw_finding
        expect: refused
        arguments:
          id: real-one
          reason: "   "
    outcome: { type: completed, summary: "reviewed" }
"#;
    let (terminal, _) = bench.run(REVIEW_NODE, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let report = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::FindingRefused(p)) => Some(p.report.clone()),
            _ => None,
        })
        .expect("the refusal is on the log");
    assert_eq!(report.diagnostics[0].problem.code(), "empty-reason");

    // The finding it named still stands.
    let bytes = bench.artifact("review.yaml").expect("the engine wrote it");
    let file: yunta_core::FindingsFile =
        yunta_core::shape::read(&bytes, "review.yaml").expect("a canonical findings file");
    assert_eq!(file.findings.len(), 1);
}

#[tokio::test]
async fn findings_reported_before_a_session_dies_survive_it() {
    let bench = Bench::new();
    let fixture = r#"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_post_finding
        arguments:
          id: found-it
          severity: blocking
          title: "reported before the end"
          location: "src/a.rs:1"
          detail: "d"
    outcome: { type: crash }
"#;
    let (terminal, state) = bench.run(REVIEW_NODE, fixture).await;
    assert!(matches!(terminal, RunTerminal::Paused { .. }), "{state:?}");

    // The session never reached an answer, and the finding is still the
    // run's: reporting it as it was seen is what makes it survive.
    assert_eq!(kinds(&bench, "finding_posted"), 1);
}
