//! What an exact fence claims, and what the diff finds past it. A breach is
//! filed only where a session claimed an exact fence and a write reached the
//! diff anyway; under widened or sandboxed coverage the same write is a
//! violation and nothing more.

use yunta_core::events::{EventPayload, Finding, FindingEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// An adapter that says it judged every write before it happened, and a write
/// that reached the diff anyway: that is something to look at in the adapter,
/// filed beside the failure the violation causes either way.
#[tokio::test]
async fn a_write_that_escapes_an_exact_fence_is_an_engine_finding() {
    // No fence judges the write, so it lands. A read-only node claims exact
    // coverage, and the diff audits it against no scope at all.
    let bench = Bench::new().with_capabilities(yunta_core::Capabilities {
        run_tools: true,
        permission_profiles: true,
        ..Default::default()
    });
    let workflow = r#"
name: scoped
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
    permissions: read-only
"#;
    // A fixture that writes outside the scope anyway: the hook timed out,
    // which this CLI lets through.
    let fixture = r#"
dialect: claude-code
effects:
  - { path: b.txt, content: "outside the scope\n" }
outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the violation fails the node and pauses the run: {terminal:?}"
    );

    assert!(
        breaches(&bench).len() == 1,
        "one breach, naming what escaped: {:?}",
        breaches(&bench)
    );
    assert!(
        breaches(&bench)[0].detail.contains("b.txt"),
        "the finding names the path that reached the diff: {:?}",
        breaches(&bench)[0]
    );
}

/// A breach is filed only when a session claims an exact fence and a write
/// reaches the diff anyway. A workspace-write session claims its worktree as
/// widened roots instead, so the same write is a violation and nothing more.
#[tokio::test]
async fn a_write_that_escapes_a_widened_coverage_is_a_violation_and_no_breach() {
    // No fence judges the write, so it lands and the post-check diff finds it.
    let bench = Bench::new().with_capabilities(yunta_core::Capabilities {
        run_tools: true,
        ..Default::default()
    });
    let workflow = r#"
name: scoped
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
    scope: ["a.txt"]
"#;
    // A fixture that writes outside the scope anyway: the hook timed out,
    // which this CLI lets through.
    let fixture = r#"
dialect: claude-code
effects:
  - { path: b.txt, content: "outside the scope\n" }
outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the violation fails the node and pauses the run: {terminal:?}"
    );

    assert!(
        breaches(&bench).is_empty(),
        "a widened coverage claims no judgement of each write, so no breach: {:?}",
        breaches(&bench)
    );
}

/// Under a widened or tool-only coverage a violation is what it always
/// was: the node fails with the list, and nobody claimed more.
#[tokio::test]
async fn a_write_inside_widened_roots_is_a_scope_violation_and_nothing_more() {
    // A filesystem sandbox judges writes by directory alone, so the write inside the worktree lands.
    let bench = Bench::new().with_capabilities(yunta_core::Capabilities {
        run_tools: true,
        fence: yunta_core::FenceLevel::Filesystem,
        ..Default::default()
    });
    let workflow = r#"
name: scoped
nodes:
  - id: work
    kind: prompt
    runner: executor
    prompt: "Do the thing."
    scope: ["a.txt"]
"#;
    let fixture = r#"
dialect: claude-code
effects:
  - { path: b.txt, content: "outside the scope\n" }
outcome: { type: completed, summary: "done" }
"#;
    let RunReport { terminal, .. } = bench.run(workflow, fixture).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "the violation fails the node and pauses the run: {terminal:?}"
    );
    assert!(
        breaches(&bench).is_empty(),
        "a sandbox never claimed to judge by glob: {:?}",
        breaches(&bench)
    );
}

/// Every fence breach the run filed.
fn breaches(bench: &Bench) -> Vec<Finding> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Posted(p)))
                if p.finding.id == yunta_engine::Breach::ID =>
            {
                Some(p.finding.clone())
            }
            _ => None,
        })
        .collect()
}
