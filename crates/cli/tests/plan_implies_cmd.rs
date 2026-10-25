//! A plan that widens a shape other code names, put in front of the person
//! approving it: every page that draws the plan says who else names the
//! shape and what the plan changes of it, and `status --json` carries what
//! the tree held of it as data.

use yunta_testkit::{run_id_from, stderr, stdout, yunta_in, yunta_on_terminal, Checkout, Terminal};

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

const RUN: [&str; 7] = [
    "run",
    "wf.yaml",
    "--adapter",
    "mock",
    "--fixture",
    "fixture.yaml",
    "--quiet",
];

/// What every page that draws the plan says of `build_it`.
const SAID: [&str; 4] = [
    "build_it is named in 1 file no task changes:",
    "src/other.rs",
    "{+width: u32+}",
    "modified",
];

/// A repository whose `src/lib.rs` declares `build_it` and whose
/// `src/other.rs` calls it.
fn checkout(root: &std::path::Path) -> Checkout {
    Checkout::under(root)
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", REVIEWED_PLAN)
        .file("fixture.yaml", PLANNER)
        .file("src/lib.rs", "pub fn build_it(depth: u32) {}\n")
        .file("src/other.rs", "fn caller() { build_it(1); }\n")
        .committed()
}

/// The run parked at its gate, by its id.
fn parked(checkout: &Checkout) -> String {
    let parked = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    assert_eq!(parked.status.code(), Some(3), "{}", stderr(&parked));
    run_id_from(&parked)
}

/// Asserts `page` says everything [`SAID`] holds.
fn says_what_the_shape_implies(page: &str) {
    for said in SAID {
        assert!(page.contains(said), "the page says `{said}`:\n{page}");
    }
}

#[test]
fn the_gate_says_what_the_plan_s_shape_implies() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let terminal: Terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &RUN);
    terminal.wait_for("3  abort", "the gate never put its options on the console");

    says_what_the_shape_implies(&terminal.drawn());
}

#[test]
fn resolve_gate_says_what_the_plan_s_shape_implies() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let run_id = parked(&checkout);

    let terminal = yunta_on_terminal!(&checkout.repo, &checkout.home, &["resolve-gate", &run_id]);
    terminal.wait_for(
        "2  adjust",
        "the run's own menu was never put to the terminal",
    );

    says_what_the_shape_implies(&terminal.drawn());
}

#[test]
fn status_node_says_what_the_plan_s_shape_implies() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let run_id = parked(&checkout);

    let shown = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["status", &run_id, "--node", "plan"]
    );
    assert!(shown.status.success(), "{}", stderr(&shown));

    says_what_the_shape_implies(&stdout(&shown));
}

#[test]
fn status_json_carries_what_the_tree_held_of_the_plan_s_shapes() {
    let root = tempfile::tempdir().unwrap();
    let checkout = checkout(root.path());
    let run_id = parked(&checkout);

    let status = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["status", &run_id, "--json"]
    );
    let document: serde_json::Value = serde_json::from_str(&stdout(&status))
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}\n{}", stdout(&status)));

    assert_eq!(
        document["shapes_read"][0]["shapes"][0]["in_tree"],
        serde_json::json!({
            "type": "there",
            "code": "pub fn build_it(depth: u32);",
            "outside": { "type": "files", "paths": ["src/other.rs"] }
        }),
        "{document:#}"
    );
}
