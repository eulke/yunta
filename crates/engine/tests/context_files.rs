//! [`check_context_files`]: whether the `files:` a workflow's nodes read
//! are in the tree a run would start from — said before a run spends
//! anything, because the run itself only finds out once every node ahead
//! of the reader has. A file nothing that runs before its reader can
//! write is refused; one an earlier node might write is warned about.

use std::collections::HashSet;
use std::path::Path;

use tokio_util::sync::CancellationToken;
use yunta_core::{CommitSha, Isolation, NodeId, Workflow};
use yunta_engine::process::Supervision;
use yunta_engine::{
    check_context_files, CheckError, CheckWarning, ContextFilesCheck, MissingContextFile,
    RunTreeOrigin,
};
use yunta_testkit::{git, git_output, init_repo, write};
use yunta_testkit_core::FixedClock;

/// Two nodes reading one file each, and one path only a run can render.
const READS: &str = r#"
name: reads
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Plan."
    context:
      - files: ["docs/architecture.md", "{{inputs.path}}"]
  - id: review
    kind: prompt
    runner: planner
    depends_on: [plan]
    prompt: "Review."
    context:
      - files: ["docs/review.md"]
"#;

fn repo() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    init_repo(root.path());
    write(
        &root.path().join("docs/architecture.md"),
        "# Architecture\n",
    );
    git(root.path(), &["add", "."]);
    git(root.path(), &["commit", "-q", "-m", "architecture"]);
    root
}

/// What the check finds for `workflow`, run from `checkout` against the
/// commit it is on.
async fn found(
    workflow: &str,
    checkout: &Path,
    mode_nodes: Option<&HashSet<NodeId>>,
    isolation: Isolation,
) -> ContextFilesCheck {
    let workflow: Workflow = yunta_core::yaml::parse(workflow).unwrap();
    let base: CommitSha = git_output(checkout, &["rev-parse", "HEAD"])
        .trim()
        .parse()
        .unwrap();
    let cancel = CancellationToken::new();
    let clock = FixedClock;
    check_context_files(
        &workflow,
        mode_nodes,
        RunTreeOrigin {
            checkout,
            isolation,
            base: &base,
        },
        Supervision::outside_any_run(&cancel, &clock),
    )
    .await
}

/// `(node, path, how)` for every path the check warns about in [`READS`],
/// where each reader has a node before it that might write the file.
async fn missing(
    checkout: &Path,
    mode_nodes: Option<&HashSet<NodeId>>,
    isolation: Isolation,
) -> Vec<(String, String, MissingContextFile)> {
    let found = found(READS, checkout, mode_nodes, isolation).await;
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    found
        .warnings
        .into_iter()
        .map(|warning| match warning {
            CheckWarning::ContextFileMissing {
                node,
                path,
                missing,
                ..
            } => (node.to_string(), path, missing),
            other => panic!("only missing files are said here, got {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn a_committed_file_says_nothing_and_a_missing_one_names_its_node() {
    let root = repo();
    // From a directory below the top: a run's paths are relative to the
    // top of the tree, wherever it was started from.
    let below = root.path().join("docs");

    assert_eq!(
        missing(&below, None, Isolation::Worktree).await,
        vec![(
            "review".to_string(),
            "docs/review.md".to_string(),
            MissingContextFile::Nowhere
        )],
        "the committed file is there and the rendered path is the run's to find"
    );
}

#[tokio::test]
async fn a_file_only_in_the_checkout_is_named_as_uncommitted() {
    let root = repo();
    write(&root.path().join("docs/review.md"), "# Review\n");

    assert_eq!(
        missing(root.path(), None, Isolation::Worktree).await,
        vec![(
            "review".to_string(),
            "docs/review.md".to_string(),
            MissingContextFile::Uncommitted
        )]
    );
}

#[tokio::test]
async fn a_file_git_ignores_is_named_as_ignored() {
    let root = repo();
    write(&root.path().join(".gitignore"), "docs/review.md\n");
    git(root.path(), &["add", ".gitignore"]);
    git(root.path(), &["commit", "-q", "-m", "ignore"]);
    write(&root.path().join("docs/review.md"), "# Review\n");

    assert_eq!(
        missing(root.path(), None, Isolation::Worktree).await,
        vec![(
            "review".to_string(),
            "docs/review.md".to_string(),
            MissingContextFile::Ignored
        )]
    );
}

#[tokio::test]
async fn a_run_in_place_reads_the_checkout_itself() {
    // `isolation: none` works in the checkout, where an uncommitted file
    // is exactly what the node reads.
    let root = repo();
    write(&root.path().join("docs/review.md"), "# Review\n");

    assert!(missing(root.path(), None, Isolation::None).await.is_empty());
}

#[tokio::test]
async fn only_the_nodes_the_mode_includes_are_checked() {
    let root = repo();
    let plan_only: HashSet<NodeId> = [NodeId::from("plan")].into_iter().collect();

    assert!(missing(root.path(), Some(&plan_only), Isolation::Worktree)
        .await
        .is_empty());
}

#[tokio::test]
async fn an_optional_entry_is_never_warned_about() {
    // Its author already said the node goes on without it.
    let root = repo();
    let workflow: Workflow = yunta_core::yaml::parse(
        r#"
name: optional
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Plan."
    context:
      - files: [{ path: docs/missing.md, optional: true }]
"#,
    )
    .unwrap();
    let base: CommitSha = git_output(root.path(), &["rev-parse", "HEAD"])
        .trim()
        .parse()
        .unwrap();
    let cancel = CancellationToken::new();
    let clock = FixedClock;
    let found = check_context_files(
        &workflow,
        None,
        RunTreeOrigin {
            checkout: root.path(),
            isolation: Isolation::Worktree,
            base: &base,
        },
        Supervision::outside_any_run(&cancel, &clock),
    )
    .await;
    assert!(
        found.warnings.is_empty() && found.errors.is_empty(),
        "{found:?}"
    );
}

/// `review` reads `docs/review.md`, which the commit does not hold, after
/// a `plan` node spliced in as `plan_lines`.
fn review_after(plan_lines: &str) -> String {
    format!(
        r#"
name: reads
nodes:
  - id: plan
{plan_lines}
  - id: review
    kind: prompt
    runner: planner
    depends_on: [plan]
    prompt: "Review."
    context:
      - files: ["docs/review.md"]
"#
    )
}

/// The paths a check refuses, by node.
fn refused(found: &ContextFilesCheck) -> Vec<(String, String)> {
    found
        .errors
        .iter()
        .map(|error| match error {
            CheckError::ContextFileUnreachable { node, path, .. } => {
                (node.to_string(), path.clone())
            }
            other => panic!("only missing files are said here, got {other:?}"),
        })
        .collect()
}

/// Nothing runs before a lone reader, so nothing can write its file: it
/// stops there every time.
#[tokio::test]
async fn a_file_nothing_before_its_reader_can_write_is_refused() {
    let root = repo();
    let alone = r#"
name: reads
nodes:
  - id: review
    kind: prompt
    runner: planner
    prompt: "Review."
    context:
      - files: ["docs/review.md"]
"#;
    let found = found(alone, root.path(), None, Isolation::Worktree).await;
    assert_eq!(
        refused(&found),
        vec![("review".to_string(), "docs/review.md".to_string())]
    );
    assert!(found.warnings.is_empty(), "{:?}", found.warnings);
    let text = found.errors[0].to_string();
    assert!(
        text.contains("stops there every time") && text.contains("optional: true"),
        "the refusal says why and the way out: {text}"
    );
}

/// What decides it is whether a node ahead of the reader can write the
/// file: a read-only session cannot, a scope that does not reach the file
/// cannot, a hook might.
#[tokio::test]
async fn whether_an_earlier_node_can_write_the_file_decides() {
    let root = repo();
    let session = "    kind: prompt\n    runner: planner\n    prompt: \"Plan.\"";
    for (plan, refused_here) in [
        (format!("{session}\n    permissions: read-only"), true),
        (format!("{session}\n    scope: [\"src/**\"]"), true),
        (format!("{session}\n    scope: [\"docs/**\"]"), false),
        (session.to_string(), false),
        ("    kind: gate\n    assignee: lead".to_string(), true),
        (
            format!(
                "{session}\n    permissions: read-only\n    hooks: {{ after: [{{ run: \"make docs\" }}] }}"
            ),
            false,
        ),
    ] {
        let found = found(&review_after(&plan), root.path(), None, Isolation::Worktree).await;
        assert_eq!(
            !found.errors.is_empty(),
            refused_here,
            "`{plan}`: {found:?}"
        );
        assert_eq!(found.warnings.is_empty(), refused_here, "`{plan}`: {found:?}");
    }
}

/// A mode that leaves the only writer out leaves the reader with nothing
/// ahead of it.
#[tokio::test]
async fn a_mode_that_drops_the_only_writer_is_refused() {
    let root = repo();
    let review_only: HashSet<NodeId> = [NodeId::from("review")].into_iter().collect();
    let found = found(READS, root.path(), Some(&review_only), Isolation::Worktree).await;
    assert_eq!(
        refused(&found),
        vec![("review".to_string(), "docs/review.md".to_string())]
    );
}

#[test]
fn the_warning_names_the_commit_the_run_starts_from_and_the_way_out() {
    let warning = CheckWarning::ContextFileMissing {
        node: NodeId::from("plan"),
        path: "docs/architecture.md".to_string(),
        base: Some("36096d2c0a1b".to_string()),
        missing: MissingContextFile::Nowhere,
    };
    assert_eq!(
        warning.to_string(),
        "node `plan` reads `docs/architecture.md` (a `files:` context source), which commit \
         `36096d2c0a1b` — the one a run starts from — does not hold: unless a node before it \
         writes the file, `plan` stops there; commit the file first, or declare the entry \
         `optional: true` if the node can do without it"
    );
}
