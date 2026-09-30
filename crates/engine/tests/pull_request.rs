//! `kind: pull_request` pushes the run's own branch and opens a pull
//! request of it into the project's base branch through the forge the
//! project configures — the same one when it runs again.

use std::sync::Arc;

use yunta_adapters::{MockForge, MockForgeState};
use yunta_core::events::{run_left_out, EventPayload, NodeEvent, StoredEvent};
use yunta_core::{ConfigKey, ConfigLayer, Workflow};
use yunta_engine::{CheckError, RunReport, RunTerminal};
use yunta_testkit::{bare_origin, git_output, Bench, INITIAL_BRANCH, MOCK_CONFIG};

const FORGE: &str = "forge:\n  github: { repo: acme/web, token_env: ACME_TOKEN }\n";

/// A node that writes, then the pull request of what it wrote.
const WRITES_THEN_OPENS: &str = r#"
name: opens
nodes:
  - { id: make, kind: bash, run: "echo made > made.txt" }
  - { id: pr, kind: pull_request, depends_on: [make], title: "add dark mode", body: "What the run did." }
"#;

fn config(extra: &str) -> String {
    format!("{MOCK_CONFIG}{FORGE}{extra}")
}

fn opened(events: &[StoredEvent]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::PullRequestOpened(p))) => Some(p.number),
            _ => None,
        })
        .collect()
}

fn forged() -> (Bench, MockForgeState) {
    let state = MockForgeState::new();
    let bench = Bench::new().with_forge(Arc::new(MockForge::new(state.clone())));
    bare_origin(&bench.worktree);
    (bench, state)
}

#[tokio::test]
async fn it_pushes_the_run_branch_and_opens_a_pr_against_the_project_base() {
    let (bench, state) = forged();
    let RunReport { terminal, .. } = bench
        .run_with_config(
            WRITES_THEN_OPENS,
            "sessions: []\n",
            &config("project:\n  base_branch: trunk\n"),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let branch = format!("yunta/run/{}", bench.run_id);
    let prs = state.pull_requests();
    assert_eq!(prs.len(), 1);
    assert_eq!(
        (
            prs[0].head.as_str(),
            prs[0].base.as_str(),
            prs[0].title.as_str()
        ),
        (branch.as_str(), "trunk", "add dark mode")
    );
    assert!(
        prs[0].body.starts_with("What the run did."),
        "{}",
        prs[0].body
    );
    let remote = bench.worktree.parent().unwrap().join("origin.git");
    assert_eq!(
        git_output(&remote, &["rev-parse", &branch]),
        git_output(&bench.worktree, &["rev-parse", "HEAD"]),
        "the remote holds everything the run committed"
    );
    assert_eq!(opened(&bench.events()), vec![prs[0].number]);
}

/// A node that runs again pushes the same branch and finds the pull
/// request it opened, instead of opening a second.
#[tokio::test]
async fn a_rerun_reuses_the_open_pr() {
    let (bench, state) = forged();
    let workflow = r#"
name: opens-twice
nodes:
  - { id: pr, kind: pull_request, title: "t", on_failure: { goto: pr, max_reroutes: 1 } }
  - { id: check, kind: bash, depends_on: [pr], run: "test -f '{{run.dir}}/once' || { touch '{{run.dir}}/once'; exit 1; }", on_failure: { goto: pr, max_reroutes: 1 } }
"#;
    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []\n", &config(""))
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.pull_requests().len(), 1);
    let numbers = opened(&bench.events());
    assert_eq!(numbers.len(), 2, "{numbers:?}");
    assert_eq!(numbers[0], numbers[1]);
}

#[tokio::test]
async fn the_base_falls_back_to_the_branch_the_run_started_from() {
    let (bench, state) = forged();
    bench
        .run_with_config(WRITES_THEN_OPENS, "sessions: []\n", &config(""))
        .await;

    assert_eq!(state.pull_requests()[0].base, INITIAL_BRANCH);
}

/// What `check` refuses `workflow` for under `config`, as `(node, key)`.
fn refusals(workflow: &str, config: &str) -> Vec<(String, ConfigKey)> {
    let workflow: Workflow = yunta_core::yaml::parse(workflow).expect("the workflow parses");
    let config: ConfigLayer = yunta_core::yaml::parse(config).expect("the config parses");
    yunta_engine::check(&workflow, &config, &|_| None)
        .into_iter()
        .filter_map(|error| match error {
            CheckError::Unset { node, key } => Some((node.to_string(), key)),
            _ => None,
        })
        .collect()
}

#[test]
fn check_refuses_it_without_a_forge_or_a_run_branch() {
    assert_eq!(
        refusals(WRITES_THEN_OPENS, "{}"),
        vec![("pr".to_string(), ConfigKey::Forge)]
    );
    assert_eq!(
        refusals(
            WRITES_THEN_OPENS,
            &format!("{FORGE}defaults:\n  isolation: none\n")
        ),
        vec![("pr".to_string(), ConfigKey::RunBranch)]
    );
    assert_eq!(refusals(WRITES_THEN_OPENS, FORGE), vec![]);
}

#[tokio::test]
async fn optional_it_is_left_out_without_a_forge() {
    let bench = Bench::new();
    let workflow = WRITES_THEN_OPENS.replace(
        "title: \"add dark mode\"",
        "title: \"add dark mode\", optional: true",
    );
    let RunReport { terminal, .. } = bench
        .run_with_config(&workflow, "sessions: []\n", MOCK_CONFIG)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let left: Vec<String> = run_left_out(&bench.events())
        .iter()
        .map(|left| left.node.to_string())
        .collect();
    assert_eq!(left, ["pr"]);
}

/// The config declares a forge this machine could not reach: the node
/// fails saying so, and a person who sets the token runs it again.
#[tokio::test]
async fn a_forge_the_machine_cannot_reach_fails_the_node_saying_why() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_config(WRITES_THEN_OPENS, "sessions: []\n", &config(""))
        .await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert!(reason.contains("token_env"), "{reason}")
        }
        other => panic!("expected the run to pause, got {other:?}"),
    }
}
