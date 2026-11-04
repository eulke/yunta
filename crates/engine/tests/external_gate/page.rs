//! A published gate reads as its page: a title that names the change, or the
//! decision when none is named, its files in the folder the run gives the gate,
//! and the same pull request redrawn after a correction.

use std::path::Path;

use surco::golden::assert_golden;
use yunta_core::events::{Claim, EventPayload, GateEvent};
use yunta_core::port::ReviewComment;
use yunta_core::{Capabilities, NodeId};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

use super::{bench_on_a_forge, published, PLANNER, PLAN_UNDER_REVIEW, TASKS_MD};

/// A plan an external gate publishes, with no message of its own to ask.
const PUBLISHED_PLAN: &str = r#"
name: published-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve
    kind: gate
    assignee: reviewer
    depends_on: [plan]
    external:
      kind: pull_request
      artifacts: [tasks]
      branch: "{{run.branch}}"
"#;

/// The claim of each gate the bench's log records as waiting, oldest first: the
/// question a person was asked.
fn waiting_claims(bench: &Bench) -> Vec<Claim> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(gate))) => Some(gate.summary().clone()),
            _ => None,
        })
        .collect()
}

/// A reviewer reads a published gate as its page: a title that names the change,
/// its files in the folder the run gives the gate and not at the branch's root, and
/// the same pull request redrawn after a correction.
#[tokio::test]
async fn the_published_gate_reads_as_its_page() {
    let (bench, forge_state) = bench_on_a_forge();
    let RunReport { terminal, .. } = bench.run(PLAN_UNDER_REVIEW, PLANNER).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let pull_requests = forge_state.pull_requests();
    assert_eq!(pull_requests.len(), 1);
    assert_eq!(pull_requests[0].title, "Approve: Make it");
    assert!(
        !pull_requests[0].files.contains_key("tasks.md"),
        "the gate's files sit in its folder, not at the branch's root: {:?}",
        pull_requests[0].files.keys().collect::<Vec<_>>()
    );
    let drawn = published(&forge_state, TASKS_MD);
    assert!(drawn.contains("test -f made.txt"), "{drawn}");

    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/forge");
    assert_golden(&goldens.join("gate.md"), &pull_requests[0].body);
    assert_golden(&goldens.join("gate-tasks.md"), &drawn);

    // A correction lap redraws the page on the same pull request.
    let bench = bench.with_capabilities(Capabilities {
        run_tools: true,
        ..Default::default()
    });
    forge_state.request_changes(
        bench.run_id.as_str(),
        &"person-b".into(),
        vec![ReviewComment {
            author: "person-b".to_string(),
            body: "split the greeting into its own task".to_string(),
            path: None,
        }],
    );
    bench.wake().await;

    let pull_requests = forge_state.pull_requests();
    assert_eq!(
        pull_requests.len(),
        1,
        "the correction reuses the pull request"
    );
    assert!(
        pull_requests[0].body.contains("T002"),
        "{}",
        pull_requests[0].body
    );
    assert!(published(&forge_state, TASKS_MD).contains("Say it"));
}

/// A gate whose plan names no change still publishes it. With no change to approve,
/// the forge titles the pull request as a decision, and the log records the review
/// the forge holds.
#[tokio::test]
async fn a_gate_whose_plan_names_no_change_titles_its_pull_request_as_a_decision() {
    let (bench, forge_state) = bench_on_a_forge();
    let planner = PLANNER.replace("            summary: \"Make it\"\n", "");
    let RunReport { terminal, .. } = bench.run(PUBLISHED_PLAN, &planner).await;
    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );

    let pull_requests = forge_state.pull_requests();
    assert_eq!(pull_requests.len(), 1);
    assert_eq!(pull_requests[0].title, "gate `approve` needs a decision");
    assert_eq!(
        waiting_claims(&bench),
        [Claim::AwaitsReview {
            node: NodeId::from_static("approve"),
            assignee: Some("reviewer".to_string()),
        }],
        "the log records the review the forge holds, and the forge titles the decision"
    );
}
