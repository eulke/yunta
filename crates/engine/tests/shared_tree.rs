//! A node that works in the run's own tree has its work committed on the
//! run's branch when it closes: what a later node pushes is what the run
//! did, and the tree reads clean after every close.

use yunta_core::events::{EventPayload, NodeEvent, StoredEvent};
use yunta_core::CommitSha;
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{git_output, Bench, MOCK_CONFIG};

/// The commit each close of `node` made, oldest first.
fn commits(events: &[StoredEvent], node: &str) -> Vec<Option<CommitSha>> {
    events
        .iter()
        .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::Finished(p))) => Some(p.commit.clone()),
            Some(EventPayload::Node(NodeEvent::Failed(p))) => Some(p.commit.clone()),
            _ => None,
        })
        .collect()
}

fn files_at_head(bench: &Bench) -> String {
    git_output(&bench.worktree, &["ls-tree", "-r", "--name-only", "HEAD"])
}

/// The symptom this exists for: a node with no scope edits code, and the
/// node after it pushes the branch.
#[tokio::test]
async fn an_unscoped_nodes_edit_reaches_the_branch_a_later_node_pushes() {
    let bench = Bench::new();
    let world = bench
        .worktree
        .parent()
        .expect("the worktree sits in the bench's world");
    git_output(world, &["init", "-q", "--bare", "remote.git"]);
    let remote = world.join("remote.git");
    let workflow = format!(
        r#"
name: pushes
nodes:
  - {{ id: fix, kind: bash, run: "echo fixed > fixed.txt" }}
  - {{ id: pr, kind: bash, depends_on: [fix], run: "git push -q '{}' HEAD:refs/heads/shipped" }}
"#,
        remote.display()
    );

    let RunReport { terminal, .. } = bench.run(&workflow, "sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    let pushed = git_output(
        &remote,
        &["ls-tree", "-r", "--name-only", "refs/heads/shipped"],
    );
    assert!(pushed.lines().any(|path| path == "fixed.txt"), "{pushed}");
}

#[tokio::test]
async fn the_run_branch_holds_every_nodes_work_and_the_tree_reads_clean() {
    let bench = Bench::new();
    let workflow = r#"
name: writes
nodes:
  - { id: first, kind: bash, run: "echo one > one.txt" }
  - { id: second, kind: bash, depends_on: [first], run: "echo two > two.txt" }
"#;
    bench.run(workflow, "sessions: []\n").await;

    let events = bench.events();
    assert!(matches!(commits(&events, "first")[..], [Some(_)]));
    assert!(matches!(commits(&events, "second")[..], [Some(_)]));
    let files = files_at_head(&bench);
    assert!(
        files.contains("one.txt") && files.contains("two.txt"),
        "{files}"
    );
    assert_eq!(git_output(&bench.worktree, &["status", "--porcelain"]), "");
    assert_eq!(
        git_output(&bench.worktree, &["log", "-1", "--format=%s"]).trim(),
        "node second: second"
    );
}

#[tokio::test]
async fn an_unscoped_node_that_changed_nothing_makes_no_commit() {
    let bench = Bench::new();
    let before = git_output(&bench.worktree, &["rev-parse", "HEAD"]);
    bench
        .run(
            "name: quiet\nnodes:\n  - { id: look, kind: bash, run: \"true\" }\n",
            "sessions: []\n",
        )
        .await;

    assert_eq!(commits(&bench.events(), "look"), vec![None]);
    assert_eq!(git_output(&bench.worktree, &["rev-parse", "HEAD"]), before);
}

/// A failed attempt's partial work is its own, committed as such, so the
/// next attempt — or a corrective node — starts from it and nothing later
/// is blamed for it.
#[tokio::test]
async fn a_failed_attempts_work_is_committed_naming_that_attempt() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(
            "name: fails\nnodes:\n  - { id: broke, kind: bash, run: \"echo half > half.txt; exit 1\" }\n",
            "sessions: []\n",
        )
        .await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert!(matches!(commits(&bench.events(), "broke")[..], [Some(_)]));
    assert_eq!(
        git_output(&bench.worktree, &["log", "-1", "--format=%s"]).trim(),
        "node broke: what attempt 1 left when it failed"
    );
}

/// Two children of a group write in one tree at once: neither can tell
/// its work from the other's, so neither commits, and the group commits
/// both when it closes, naming them.
#[tokio::test]
async fn two_unscoped_children_of_a_group_are_committed_once_when_the_group_closes() {
    let bench = Bench::new();
    let meet = |mine: &str, theirs: &str| {
        format!(
            "mkdir -p '{{{{run.dir}}}}/meet' && touch '{{{{run.dir}}}}/meet/{mine}' && \
             while [ ! -f '{{{{run.dir}}}}/meet/{theirs}' ]; do sleep 0.05; done; \
             echo {mine} > {mine}.txt"
        )
    };
    let workflow = format!(
        r#"
name: grouped
nodes:
  - id: group
    kind: parallel
    nodes:
      - {{ id: a, kind: bash, run: "{}" }}
      - {{ id: b, kind: bash, run: "{}" }}
"#,
        meet("a", "b"),
        meet("b", "a")
    );
    let RunReport { terminal, .. } = bench.run(&workflow, "sessions: []\n").await;

    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.events();
    assert_eq!(commits(&events, "a"), vec![None]);
    assert_eq!(commits(&events, "b"), vec![None]);
    assert!(matches!(commits(&events, "group")[..], [Some(_)]));
    let message = git_output(&bench.worktree, &["log", "-1", "--format=%B"]);
    assert!(
        message.contains("`a`") && message.contains("`b`"),
        "{message}"
    );
    let files = files_at_head(&bench);
    assert!(
        files.contains("a.txt") && files.contains("b.txt"),
        "{files}"
    );
}

#[tokio::test]
async fn what_git_ignores_stays_out_of_the_commit() {
    let bench = Bench::new();
    tokio::fs::write(bench.worktree.join(".gitignore"), "build/\n")
        .await
        .unwrap();
    bench
        .run(
            "name: builds\nnodes:\n  - { id: make, kind: bash, run: \"mkdir -p build && echo bin > build/out.bin && echo src > src.txt\" }\n",
            "sessions: []\n",
        )
        .await;

    let files = files_at_head(&bench);
    assert!(files.contains("src.txt"), "{files}");
    assert!(!files.contains("build/out.bin"), "{files}");
    assert!(
        bench.worktree.join("build/out.bin").exists(),
        "still in the tree"
    );
}

/// A run working in a person's own checkout leaves their branch as it
/// found it: what its nodes write stays uncommitted for the person.
#[tokio::test]
async fn a_run_without_isolation_commits_none_of_its_nodes_work() {
    let bench = Bench::new();
    let before = git_output(&bench.worktree, &["rev-parse", "HEAD"]);
    let config = format!("{MOCK_CONFIG}defaults:\n  isolation: none\n");
    bench
        .run_with_config(
            "name: in-place\nnodes:\n  - { id: fix, kind: bash, run: \"echo fixed > fixed.txt\" }\n",
            "sessions: []\n",
            &config,
        )
        .await;

    assert_eq!(commits(&bench.events(), "fix"), vec![None]);
    assert_eq!(git_output(&bench.worktree, &["rev-parse", "HEAD"]), before);
}
