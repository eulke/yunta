//! A resume verifies that the worktree is still the tree the run's own
//! history describes — its identity and its ancestry, never its content.
//!
//! The content of a worktree is the work: a person who opens a paused
//! run's checkout, fixes something by hand, runs the tests and commits is
//! using the system exactly as designed, and the engine answers a tree
//! that moved by re-running the criteria against it. What a resume checks
//! is what cannot legitimately change: that there is a working tree where
//! the manifest froze one, and that the commit the run branched from is
//! still behind its HEAD. A tree that has lost that commit leaves the run
//! `broken`: every task done, scope checked and criterion green on its log
//! describes a tree this one does not continue.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use yunta_core::events::{EventDraft, EventPayload, NodeStartedPayload};
use yunta_core::{ConfigLayer, Isolation, Manifest, SystemClock, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, run_branch, CreateRunParams, NoInteraction, RunEnv,
    RunError, RunReport, RunTerminal, WorktreeError, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{git, git_output, Bench, FixedClock, MOCK_CONFIG};

mod common;
use common::*;

/// One node the engine finds interrupted with no terminal event, whose
/// policy is to pause rather than guess — so every invocation of this run
/// is a resume that stops again.
const ONE_INTERRUPTED_NODE: &str = r#"
name: works-in-a-worktree
nodes:
  - id: only
    kind: bash
    on_interrupt: fail_if_uncertain
    run: "true"
"#;

/// `isolation: none` on top of the runners every mock-backed run needs.
const NONE_ISOLATION: &str = "defaults:\n  isolation: none\n";

/// A run paused mid-node whose worktree is a real git checkout: the state
/// a resume verifies.
struct Paused {
    bench: Bench,
    manifest: Manifest,
    run_dir: PathBuf,
}

impl Paused {
    /// Wakes the run again — the resume under test.
    async fn resume(&self) -> Result<RunReport, RunError> {
        execute_run(RunEnv {
            run_id: &self.bench.run_id,
            manifest: &self.manifest,
            run_dir: &self.run_dir,
            worktree: &self.bench.worktree,
            adapters: &HashMap::new(),
            storage: &self.bench.storage.async_handle(),
            clock: std::sync::Arc::new(FixedClock),
            ids: &IDS,
            max_task_retries: DEFAULT_MAX_RETRIES,
            human_interaction: &NoInteraction,
            forge: None,
            cancel: None,
            adapter_override: None,
            ambient: None,
        })
        .await
    }

    fn worktree(&self) -> &Path {
        &self.bench.worktree
    }

    /// The commit the run branched from, frozen in its manifest.
    fn base_commit(&self) -> String {
        self.manifest.base_commit.to_string()
    }
}

/// Builds that run. The worktree already carries a commit *before* the
/// one the run branches from, so a test can put it back behind the run's
/// own base and see what the resume makes of that.
async fn paused_run(isolation: Isolation) -> Paused {
    let bench = Bench::new();
    git(
        &bench.worktree,
        &["commit", "-q", "--allow-empty", "-m", "before the run"],
    );

    let workflow: Workflow = serde_norway::from_str(ONE_INTERRUPTED_NODE).unwrap();
    let config_text = match isolation {
        Isolation::Worktree => MOCK_CONFIG.to_string(),
        Isolation::None => format!("{MOCK_CONFIG}{NONE_ISOLATION}"),
    };
    let config: ConfigLayer = serde_norway::from_str(&config_text).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap()
    .manifest;
    assert_eq!(
        manifest.isolation, isolation,
        "the run under test is the one the test asked for"
    );
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    // A crash mid-node: `node_started` with no terminal event, under the
    // one policy that pauses instead of re-running.
    bench
        .storage
        .append(
            &EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
            },
            &SystemClock,
        )
        .unwrap();

    Paused {
        manifest,
        run_dir,
        bench,
    }
}

/// The `run_resumed` payloads on a run's log, one per invocation that
/// woke it.
fn resumes(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| matches!(e.payload(), Some(EventPayload::RunResumed(_))))
        .count()
}

/// The diagnostic of a run the resume refused to wake.
fn broken(error: &RunError) -> &str {
    match error {
        RunError::Broken { diagnostic } => diagnostic,
        other => panic!("expected a broken run, got: {other:?}"),
    }
}

#[tokio::test]
async fn a_worktree_carrying_the_runs_own_commits_resumes_without_a_word() {
    let paused = paused_run(Isolation::Worktree).await;
    git(
        paused.worktree(),
        &["commit", "-q", "--allow-empty", "-m", "what the run did"],
    );

    let report = paused
        .resume()
        .await
        .expect("the run wakes and pauses again");

    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&paused.bench), 1, "the run woke normally");
    assert!(
        report.state.findings.is_empty(),
        "a tree that moved forward is the expected case: {:?}",
        report.state.findings
    );
}

/// The worktree is the work, so its content is never verified: a person
/// who opens a paused run's checkout and fixes something by hand is using
/// the system as designed, and the run resumes over their edit. What
/// answers a changed tree is the criteria memo, which re-runs the criteria
/// against it rather than reusing a result about a tree that is gone.
#[tokio::test]
async fn a_worktree_edited_by_hand_during_the_pause_resumes_because_the_content_is_the_work() {
    let paused = paused_run(Isolation::Worktree).await;
    std::fs::write(paused.worktree().join("fixed-by-hand.rs"), "fn main() {}").unwrap();
    std::fs::write(paused.worktree().join(".gitkeep"), "touched").unwrap();

    let report = paused
        .resume()
        .await
        .expect("uncommitted work in the tree is not a reason to refuse the run");

    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&paused.bench), 1, "the run woke normally");
    assert!(
        report.state.findings.is_empty(),
        "an edited tree is not reported as anything: {:?}",
        report.state.findings
    );
}

#[tokio::test]
async fn a_worktree_reset_behind_the_runs_base_commit_leaves_the_run_broken() {
    let paused = paused_run(Isolation::Worktree).await;
    git(paused.worktree(), &["reset", "--hard", "-q", "HEAD~1"]);
    let head = git_output(paused.worktree(), &["rev-parse", "HEAD"]);

    let error = paused
        .resume()
        .await
        .expect_err("the run's history describes a tree this one does not continue");

    let diagnostic = broken(&error);
    assert!(
        diagnostic.contains(&paused.worktree().display().to_string()),
        "the diagnostic names the worktree: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&paused.base_commit()),
        "the diagnostic names the base commit: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&head),
        "the diagnostic names the HEAD it found: {diagnostic}"
    );
    assert_eq!(
        resumes(&paused.bench),
        0,
        "a run that cannot be verified never records that it resumed"
    );
}

#[tokio::test]
async fn a_worktree_moved_to_an_unrelated_branch_leaves_the_run_broken() {
    let paused = paused_run(Isolation::Worktree).await;
    git(
        paused.worktree(),
        &["checkout", "-q", "--orphan", "somewhere-else"],
    );
    git(
        paused.worktree(),
        &["commit", "-q", "--allow-empty", "-m", "another history"],
    );

    let error = paused
        .resume()
        .await
        .expect_err("a tree on an unrelated history is not the run's tree");

    let diagnostic = broken(&error);
    assert!(
        diagnostic.contains(&paused.base_commit()),
        "the diagnostic names the base commit it looked for: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&run_branch(&paused.bench.run_id)),
        "the remedy names the run's own branch: {diagnostic}"
    );
}

/// A worktree somebody deleted is not a broken run: the run's log and its
/// objects — everything that is its evidence — are untouched, and git
/// still holds its branch, so the checkout comes back with one command.
/// The error says which one.
#[tokio::test]
async fn a_worktree_that_is_gone_is_an_error_that_says_how_to_bring_it_back() {
    let paused = paused_run(Isolation::Worktree).await;
    std::fs::remove_dir_all(paused.worktree()).unwrap();

    let error = paused
        .resume()
        .await
        .expect_err("there is no tree at the path the manifest froze");

    let RunError::Worktree(WorktreeError::RunWorktreeLost { .. }) = &error else {
        panic!("a missing worktree is an error with a remedy, not a broken run: {error:?}");
    };
    let message = error.to_string();
    assert!(
        message.contains(&paused.worktree().display().to_string())
            && message.contains(&run_branch(&paused.bench.run_id)),
        "the error names the path and the branch to recreate it from: {message}"
    );
    assert!(
        message.contains("git worktree add"),
        "the error says how to bring the checkout back: {message}"
    );
    assert_eq!(
        resumes(&paused.bench),
        0,
        "a run whose tree cannot be found never records that it resumed"
    );
}

/// `isolation: none` runs on the checkout it was created in rather than on
/// a worktree of its own, and the same two questions apply: the run's
/// derived state describes that checkout's history just as much.
#[tokio::test]
async fn an_isolation_none_checkout_reset_behind_the_base_commit_leaves_the_run_broken() {
    let paused = paused_run(Isolation::None).await;
    git(paused.worktree(), &["reset", "--hard", "-q", "HEAD~1"]);

    let error = paused
        .resume()
        .await
        .expect_err("the checkout no longer has the commit the run started from");

    let diagnostic = broken(&error);
    assert!(
        diagnostic.contains(&paused.base_commit()),
        "the diagnostic names the base commit: {diagnostic}"
    );
    assert!(
        diagnostic.contains("isolation: none"),
        "the remedy is the one for a run that works on your own checkout: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(&run_branch(&paused.bench.run_id)),
        "a run with no worktree of its own has no run branch to send anyone to: {diagnostic}"
    );
}

#[tokio::test]
async fn an_isolation_none_checkout_that_moved_forward_resumes_without_a_word() {
    let paused = paused_run(Isolation::None).await;
    git(
        paused.worktree(),
        &["commit", "-q", "--allow-empty", "-m", "what the run did"],
    );
    std::fs::write(paused.worktree().join("fixed-by-hand.rs"), "fn main() {}").unwrap();

    let report = paused
        .resume()
        .await
        .expect("a checkout that carries the run's own work forward is the expected case");

    assert!(matches!(report.terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&paused.bench), 1, "the run woke normally");
}
