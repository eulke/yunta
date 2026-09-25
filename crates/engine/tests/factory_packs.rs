//! `yunta/fragua` runs its full reference pipeline end to end with the
//! `mock` adapter, `pr` included. The pack's own `.yunta/tests/` cases
//! stop at the first gate: a case has no human to answer a gate and no
//! `gh` to call. This test drives `mode: quick` directly through the
//! engine, approving `ship` the way an operator's own automation would
//! and stubbing `gh` so the `pr` node's real command has something to
//! call.

use std::path::Path;

use yunta_engine::{run_branch, NodeState, RunReport, RunTerminal};
use yunta_testkit::{git, write, ApproveEverything, Bench, INITIAL_BRANCH, MOCK_CONFIG};

/// The pack directory the workflow, its `prompt: { file: … }` and its
/// provenance are read from.
const WORKFLOWS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packs/fragua/.yunta/workflows"
);

/// One scripted session per node the "quick" mode actually spawns, in
/// the order the DAG reaches them: grill, brief, plan, one implement
/// task, then lint/tests/ship/pr run for real against the sandbox crate
/// this test lays down (no mock involved — cargo and git are the real
/// things being exercised, exactly as they would be in production). The
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
            tasks:
              - id: T001
                title: "Document the sandbox crate"
                scope: ["src/lib.rs"]
                criteria:
                  - cmd: "grep -q '//! sandbox' src/lib.rs"
    outcome: { type: completed, summary: "planned" }
  - effects:
      - path: "src/lib.rs"
        content: |
          //! sandbox

          pub fn hello() -> &'static str {
              "hello"
          }
    outcome: { type: completed, summary: "did T001" }
"##;

#[tokio::test]
async fn yunta_fragua_runs_end_to_end_in_quick_mode_with_mock() {
    // `gh` isn't installed in this environment (or anywhere CI runs) —
    // stub it so the `pr` node's real bash command has something to
    // call, and inject the stub's directory onto the run's subprocess
    // `PATH` so every governed child finds it without this test mutating
    // its own process environment.
    let stub_dir = tempfile::tempdir().unwrap();
    let gh_stub = stub_dir.path().join("gh");
    write(&gh_stub, "#!/bin/sh\necho \"pr created (stub): $*\"\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh_stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let inherited_path = std::env::var("PATH").unwrap_or_default();

    let config = format!(
        "{MOCK_CONFIG}project:\n  base_branch: {INITIAL_BRANCH}\nbaseline:\n  suite: \"true\"\n"
    );
    let bench = Bench::with_run_id("run-fragua")
        .in_mode("quick")
        .with_inputs(&[("idea", "add dark mode")])
        .with_workflow_dir(WORKFLOWS)
        .with_subprocess_vars(vec![(
            "PATH".to_string(),
            format!("{}:{}", stub_dir.path().display(), inherited_path),
        )]);

    write(
        &bench.worktree.join("Cargo.toml"),
        "[package]\nname = \"sandbox\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &bench.worktree.join("src/lib.rs"),
        "pub fn hello() -> &'static str {\n    \"hello\"\n}\n",
    );
    write(
        &bench.worktree.join("docs/architecture.md"),
        "# Architecture\n\nA sandbox crate for the fragua reference pipeline's own test.\n",
    );
    git(&bench.worktree, &["add", "."]);
    git(&bench.worktree, &["commit", "-q", "-m", "sandbox crate"]);

    // `pr`'s own `git push -u origin {{run.branch}}` needs a real remote
    // and a local branch of exactly that name — both of which an
    // `isolation: worktree` run gets from `prepare_worktree` before any
    // node executes. A bench executes against the checkout it is handed,
    // so the remote and the branch are laid down here.
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare"]);
    git(
        &bench.worktree,
        &["remote", "add", "origin", origin.path().to_str().unwrap()],
    );
    git(
        &bench.worktree,
        &["checkout", "-q", "-b", &run_branch(&bench.run_id)],
    );

    let workflow = std::fs::read_to_string(Path::new(WORKFLOWS).join("fragua.yaml")).unwrap();
    let RunReport { terminal, state } = bench
        .run_full(&workflow, FIXTURE, &config, &ApproveEverything::new("test"))
        .await;

    assert_eq!(terminal, RunTerminal::Finished, "state: {state:?}");
    for node in ["grill", "plan", "implement", "lint", "tests", "ship", "pr"] {
        assert!(
            matches!(state.nodes.state(node), Some(NodeState::Finished { .. })),
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
}
