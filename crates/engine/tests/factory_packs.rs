//! `yunta/fragua` runs its full reference pipeline end to end with the
//! `mock` adapter, `pr` included. The pack's own `.yunta/tests/` cases
//! stop at the first gate: a case has no human to answer a gate. This
//! test drives `mode: quick` directly through the engine, approving
//! `ship` the way an operator's own automation would, and opens the pull
//! request on a stand-in forge after pushing to a real remote.

use std::path::Path;
use std::sync::Arc;

use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::events::{run_left_out, EventPayload, NodeEvent};
use yunta_engine::{NodeState, RunReport, RunTerminal};
use yunta_testkit::{
    bare_origin, git, write, ApproveEverything, Bench, INITIAL_BRANCH, MOCK_CONFIG,
};

/// The pack directory the workflow, its `prompt: { file: … }` and its
/// provenance are read from.
const WORKFLOWS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packs/fragua/.yunta/workflows"
);

/// One scripted session per node the "quick" mode actually spawns, in
/// the order the DAG reaches them: grill, brief, plan — explained, since
/// `ship` shows it — one implement task, and `conform` finding the work
/// holds to the plan; lint/tests/ship/pr run for real against the
/// sandbox this test lays down (no mock involved — the project's lint
/// command and git are the real things being exercised, exactly as in
/// production). The
/// two interpreted documents go over the run tools; `brief.md` is a
/// session's own file, and lands in `brief`'s own directory — the
/// absolute path that session is granted, the same way a real agent
/// reads it from its rendered prompt, since a session's cwd is the
/// worktree. `grill` asks nothing, so the case runs to its end with
/// nobody to answer.
const FIXTURE: &str = r##"
capabilities: { run_tools: true }
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions: []
    outcome: { type: completed, summary: "grilled" }
  - effects:
      - path: "{{run.staging}}/brief/brief.md"
        content: "# Brief\n\nAdd dark mode.\n"
    outcome: { type: completed, summary: "brief written" }
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Document the sandbox crate"
            description: "Says what the crate is, at its top."
            tasks:
              - id: T001
                title: "Document the sandbox crate"
                description: "Adds the crate's doc comment."
                scope: ["src/lib.rs"]
                changes: [{ at: src/lib.rs, what: "the crate's doc comment", code: "//! The crate." }]
                outcome: "The crate's documentation says it is the sandbox"
                criteria:
                  - cmd: "grep -q '//! sandbox' src/lib.rs"
                    proves: "the crate says what it is"
    outcome: { type: completed, summary: "planned" }
  - effects:
      - path: "src/lib.rs"
        content: |
          //! sandbox

          pub fn hello() -> &'static str {
              "hello"
          }
    outcome: { type: completed, summary: "did T001" }
  - outcome: { type: completed, summary: "the work holds to its plan" }
"##;

/// The project a fragua run works in: a sandbox with the file `plan`
/// reads, committed, and a bare `origin` to push the run's branch to.
fn sandbox(bench: &Bench) {
    write(
        &bench.worktree.join("src/lib.rs"),
        "pub fn hello() -> &'static str {\n    \"hello\"\n}\n",
    );
    write(
        &bench.worktree.join("docs/architecture.md"),
        "# Architecture\n\nA sandbox for the fragua reference pipeline's own test.\n",
    );
    git(&bench.worktree, &["add", "."]);
    git(&bench.worktree, &["commit", "-q", "-m", "sandbox"]);
    bare_origin(&bench.worktree);
}

/// A quick-mode fragua run on a forge the test reads back, under a
/// config that adds `extra` to the runners — `conform`'s `reviewer`
/// among them — suite and forge.
async fn quick_run(extra: &str) -> (Bench, MockForgeState, RunReport) {
    let config = format!(
        "{MOCK_CONFIG}  reviewer:\n    - {{ adapter: mock, model: mock-model }}\n\
         project:\n  base_branch: {INITIAL_BRANCH}\nbaseline:\n  suite: \"true\"\n\
         forge:\n  github: {{ repo: acme/sandbox, token_env: SANDBOX_TOKEN }}\n{extra}"
    );
    let forge = MockForgeState::new();
    let bench = Bench::with_run_id("run-fragua")
        .in_mode("quick")
        .with_inputs(&[("idea", "add dark mode")])
        .with_workflow_dir(WORKFLOWS)
        .with_forge(Arc::new(MockForge::new(forge.clone())));
    sandbox(&bench);
    let workflow = std::fs::read_to_string(Path::new(WORKFLOWS).join("fragua.yaml")).unwrap();
    let report = bench
        .run_full(&workflow, FIXTURE, &config, &ApproveEverything::new("test"))
        .await;
    (bench, forge, report)
}

/// The work is held to its plan first; lint and the comparison run once,
/// after the last node that could change it.
fn checks_follow_the_work(bench: &Bench) {
    let started = started_in_order(bench);
    let at = |node: &str| started.iter().position(|seen| seen == node);
    assert!(
        at("implement") < at("conform") && at("conform") < at("lint") && at("lint") < at("tests"),
        "{started:?}"
    );
}

/// Each node, the first time the log has it start.
fn started_in_order(bench: &Bench) -> Vec<String> {
    let mut started: Vec<String> = Vec::new();
    for event in bench.events() {
        if let (Some(EventPayload::Node(NodeEvent::Started(_))), Some(node)) =
            (event.payload(), event.node_id.as_ref())
        {
            if !started.iter().any(|seen| seen == node.as_str()) {
                started.push(node.to_string());
            }
        }
    }
    started
}

fn finished(state: &yunta_engine::RunState, node: &str) -> bool {
    matches!(state.nodes.state(node), Some(NodeState::Finished { .. }))
}

#[tokio::test]
async fn yunta_fragua_runs_end_to_end_in_quick_mode_with_mock() {
    // The project's lint reads what the implement task wrote, so a pass
    // says it ran in the run's own tree after it.
    let (bench, forge, RunReport { terminal, state }) =
        quick_run("commands:\n  lint: \"grep -q '//! sandbox' src/lib.rs\"\n").await;

    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
    for node in [
        "grill",
        "plan",
        "implement",
        "lint",
        "tests",
        "conform",
        "ship",
        "pr",
    ] {
        assert!(
            finished(&state, node),
            "node `{node}` did not finish: {:?}",
            state.nodes.state(node)
        );
    }
    // `fix-lint` is only in quick mode's node set as a re-route target —
    // lint passed on the first try, so it must never have run.
    assert!(
        !state.nodes.has_state("fix-lint"),
        "fix-lint ran despite lint passing on the first try: {:?}",
        state.nodes.state("fix-lint")
    );
    checks_follow_the_work(&bench);
    let prs = forge.pull_requests();
    assert_eq!(prs.len(), 1, "{prs:?}");
    assert_eq!(
        (
            prs[0].title.as_str(),
            prs[0].head.as_str(),
            prs[0].base.as_str()
        ),
        (
            "add dark mode",
            format!("yunta/run/{}", bench.run_id).as_str(),
            INITIAL_BRANCH
        )
    );
}

#[tokio::test]
async fn yunta_fragua_leaves_lint_out_where_the_project_declares_none() {
    let (bench, forge, RunReport { terminal, state }) = quick_run("").await;

    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
    let left: Vec<String> = run_left_out(&bench.events())
        .iter()
        .map(|left| left.node.to_string())
        .collect();
    assert_eq!(left, ["lint", "fix-lint"]);
    for node in ["implement", "tests", "ship", "pr"] {
        assert!(finished(&state, node), "node `{node}` did not finish");
    }
    assert_eq!(forge.pull_requests().len(), 1);
}

/// What each node of fragua waits on in a mode, read off the workflow:
/// in `standard` the reviewers and `conform` read the work side by side
/// and one round of fixes answers both before `lint`; in `quick`, where
/// no review runs, `lint` follows `conform`.
#[test]
fn fragua_s_checks_follow_the_last_node_that_changes_the_work() {
    let text = std::fs::read_to_string(Path::new(WORKFLOWS).join("fragua.yaml")).unwrap();
    let workflow: yunta_core::Workflow = yunta_core::yaml::parse(&text).unwrap();
    let waits = |mode: &str, node: &str| -> Vec<String> {
        let included = yunta_engine::mode_included_nodes(&workflow, &mode.into());
        let mut deps: Vec<String> =
            yunta_engine::dependencies_in_mode(&workflow, included.as_ref())
                .remove(&yunta_core::NodeId::from(node))
                .unwrap_or_default()
                .iter()
                .map(ToString::to_string)
                .collect();
        deps.sort();
        deps
    };

    assert_eq!(waits("standard", "review"), vec!["implement"]);
    assert_eq!(waits("standard", "conform"), vec!["implement"]);
    assert_eq!(waits("standard", "fix-findings"), vec!["conform", "review"]);
    assert_eq!(waits("standard", "lint"), vec!["fix-findings"]);
    assert_eq!(waits("standard", "ship"), vec!["tests"]);
    assert_eq!(waits("quick", "conform"), vec!["implement"]);
    assert_eq!(waits("quick", "lint"), vec!["conform", "implement"]);
    assert_eq!(waits("quick", "ship"), vec!["tests"]);
}
