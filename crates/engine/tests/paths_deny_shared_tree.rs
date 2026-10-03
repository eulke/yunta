//! Nothing a project denies to every run reaches the run's branch from
//! its shared tree: the close that commits refuses it, puts it back as
//! the branch had it, and fails the node that answers for the commit.

use yunta_core::events::{EventPayload, Failure, NodeEvent, StoredEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git_output, write, Bench, MOCK_CONFIG};

mod common;
use common::answer_parked;

fn denying_ci() -> String {
    format!("{MOCK_CONFIG}permissions:\n  paths:\n    deny: [\".github/**\"]\n")
}

/// Each failure of `node`, with what its close refused.
fn failures(events: &[StoredEvent], node: &str) -> Vec<(Failure, Vec<std::path::PathBuf>)> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Failed(p))) => {
                Some((p.failure.clone(), p.refused.clone()))
            }
            _ => None,
        })
        .collect()
}

fn at_head(bench: &Bench) -> String {
    git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"])
}

#[tokio::test]
async fn an_unscoped_node_writing_a_denied_path_fails_and_head_never_holds_it() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, run: \"mkdir -p .github && echo x > .github/ci.yml && echo y > notes.txt\" }\n";
    let RunReport { terminal, .. } = bench
        .run_with_config(workflow, "sessions: []\n", &denying_ci())
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    let ci = std::path::PathBuf::from(".github/ci.yml");
    assert_eq!(
        failures(&bench.events(), "tidy"),
        vec![(Failure::paths_denied(vec![ci.clone()]), vec![ci])]
    );
    let head = at_head(&bench);
    assert!(head.contains("notes.txt"), "the rest is committed: {head}");
    assert!(!head.contains(".github/ci.yml"), "{head}");
    assert!(
        !bench.worktree.join(".github/ci.yml").exists(),
        "put back as the branch had it"
    );
}

/// A node that fails anyway still has its close refuse what it denied,
/// and its failure says so beside its own cause.
#[tokio::test]
async fn a_failing_node_names_the_denied_paths_its_close_refused() {
    let bench = Bench::new();
    let workflow = "name: w\nnodes:\n  - { id: tidy, kind: bash, run: \"mkdir -p .github && echo x > .github/ci.yml; exit 3\" }\n";
    bench
        .run_with_config(workflow, "sessions: []\n", &denying_ci())
        .await;

    let failed = failures(&bench.events(), "tidy");
    let [(failure, refused)] = &failed[..] else {
        panic!("one failure: {failed:?}");
    };
    assert!(
        failure.to_string().starts_with("exit 3"),
        "its own cause stands: {failure}"
    );
    assert_eq!(refused, &[std::path::PathBuf::from(".github/ci.yml")]);
    assert!(!at_head(&bench).contains(".github/ci.yml"));
}

/// What a person edits during a pause is theirs, not the run's: it is
/// committed as found even where the project denies it to every run.
#[tokio::test]
async fn a_persons_edit_to_a_denied_path_is_committed_as_found() {
    let workflow =
        "name: w\nnodes:\n  - { id: check, kind: bash, run: \"test -f .github/ci.yml\" }\n";
    let bench = Bench::new();
    let paused = bench
        .run_with_config(workflow, "sessions: []\n", &denying_ci())
        .await;
    assert!(matches!(paused.terminal, RunTerminal::Paused { .. }));
    write(
        &bench.worktree.join(".github/ci.yml"),
        "a person's workflow\n",
    );

    answer_parked(&bench, "retry").await.unwrap();
    let RunReport { terminal, .. } = bench.wake_on_fixture("sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(at_head(&bench).contains(".github/ci.yml"));
}

/// Two children writing in one tree cannot be told apart, so the group's
/// commit answers for what both wrote.
#[tokio::test]
async fn a_group_answers_for_what_its_children_wrote() {
    let bench = Bench::new();
    let workflow = r#"
name: grouped
nodes:
  - id: group
    kind: parallel
    nodes:
      - { id: a, kind: bash, run: "echo a > a.txt" }
      - { id: b, kind: bash, run: "mkdir -p .github && echo b > .github/b.yml" }
"#;
    bench
        .run_with_config(workflow, "sessions: []\n", &denying_ci())
        .await;

    let events = bench.events();
    assert!(failures(&events, "a").is_empty() && failures(&events, "b").is_empty());
    assert_eq!(
        failures(&events, "group")
            .into_iter()
            .map(|(_, refused)| refused)
            .collect::<Vec<_>>(),
        vec![vec![std::path::PathBuf::from(".github/b.yml")]]
    );
    let head = at_head(&bench);
    assert!(
        head.contains("a.txt") && !head.contains(".github/b.yml"),
        "{head}"
    );
}
