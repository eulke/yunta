//! A run's page read whole through the API: the decision a parked run waits on with
//! the documents it shows, the plans a full page tells, and a node the run does not
//! declare refused with the name it is nearest to.

mod bench_project;

use std::sync::Mutex;

use surco::doc::Depth;
use yunta_api::{Error, Remark, Remarks};
use yunta_core::NodeId;
use yunta_render::refusal::Refused;
use yunta_testkit::Bench;

use bench_project::project_of;

/// A planner, and the gate that shows its plan.
const REVIEWED_PLAN: &str = r#"
name: reviewed-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

/// The planner, handing over a plan that widens `build_it`.
const PLANNER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Widen build_it"
            description: "build_it takes a width beside its depth."
            shapes:
              - { name: build_it, owner: T001, file: src/lib.rs, code: "pub fn build_it(depth: u32, width: u32);" }
            tasks:
              - { id: T001, title: "Widen build_it", description: "Gives build_it a width.", scope: [src/lib.rs], changes: [{ at: "src/lib.rs::build_it", what: "takes a width", code: "pub fn build_it(depth: u32, width: u32);" }], outcome: "build_it takes a width", criteria: [{ cmd: "test -f built.txt", proves: "the width is built" }] }
    outcome: { type: completed, summary: "planned" }
"#;

/// What an operation said, kept in order.
#[derive(Default)]
struct Kept(Mutex<Vec<Remark>>);

impl Remarks for Kept {
    fn remark(&self, remark: Remark) {
        self.0.lock().unwrap().push(remark);
    }
}

/// A run parked on the gate that shows its plan.
async fn parked() -> Bench {
    let bench = Bench::with_run_id("parked-on-its-plan");
    bench.run(REVIEWED_PLAN, PLANNER).await;
    bench
}

#[tokio::test]
async fn a_parked_runs_page_holds_the_documents_its_decision_shows() {
    let bench = parked().await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let page = open.page(Depth::Glance, &Kept::default()).await.unwrap();
    let decision = page.decision.expect("the run waits on its gate");
    assert_eq!(decision.shown.len(), 1, "the plan the gate shows");
    assert!(page.plans.is_empty(), "a glance tells no plan");
}

#[tokio::test]
async fn a_full_page_tells_the_change_each_finished_nodes_plan_makes() {
    let bench = parked().await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let page = open.page(Depth::Full, &Kept::default()).await.unwrap();
    let nodes: Vec<&str> = page.plans.iter().map(|(node, _)| node.as_str()).collect();
    assert_eq!(nodes, ["plan"]);
}

#[tokio::test]
async fn a_node_the_run_does_not_declare_is_refused_with_the_name_it_is_nearest_to() {
    let bench = parked().await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let refused = open
        .node_page(&NodeId::from("aprove-plan"), &Kept::default())
        .await
        .err()
        .expect("no such node");
    let Error::Refused(Refused::NoSuchNode { declared, .. }) = &refused else {
        panic!("refused as no such node: {refused}");
    };
    assert!(declared.iter().any(|node| node.as_str() == "approve-plan"));
    assert!(refused.to_string().contains("`approve-plan`"), "{refused}");
}

#[tokio::test]
async fn a_nodes_page_holds_what_the_node_produced() {
    let bench = parked().await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let page = open
        .node_page(&NodeId::from("plan"), &Kept::default())
        .await
        .unwrap();
    assert_eq!(page.node.id.as_str(), "plan");
    assert_eq!(page.documents.len(), 1, "the plan the node produced");
}
