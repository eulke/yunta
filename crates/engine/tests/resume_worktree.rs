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

use yunta_core::events::{EventDraft, EventPayload, NodeEvent, NodeStartedPayload, RunEvent};
use yunta_core::{Isolation, SystemClock};
use yunta_engine::{run_branch, RunError, RunReport, RunTerminal, WorktreeError};
use yunta_testkit::{git, git_output, Bench, MOCK_CONFIG};

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
/// a resume verifies. The worktree already carries a commit *before* the
/// one the run branches from, so a test can put it back behind the run's
/// own base and see what the resume makes of that.
async fn paused_run(isolation: Isolation) -> Bench {
    let bench = Bench::new();
    git(
        &bench.worktree,
        &["commit", "-q", "--allow-empty", "-m", "before the run"],
    );

    let config = match isolation {
        Isolation::Worktree => MOCK_CONFIG.to_string(),
        Isolation::None => format!("{MOCK_CONFIG}{NONE_ISOLATION}"),
    };
    bench
        .create(ONE_INTERRUPTED_NODE, "sessions: []\n", &config)
        .await;
    assert_eq!(
        bench.manifest().isolation,
        isolation,
        "the run under test is the one the test asked for"
    );
    // A crash mid-node: `node_started` with no terminal event, under the
    // one policy that pauses instead of re-running.
    bench
        .storage
        .append(
            &EventDraft {
                run_id: bench.run_id.clone(),
                node_id: Some("only".into()),
                payload: EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
            },
            &SystemClock,
        )
        .unwrap();

    bench
}

/// The commit the run branched from, frozen in its manifest.
fn base_commit(bench: &Bench) -> String {
    bench.manifest().base_commit.to_string()
}

/// The `run_resumed` payloads on a run's log, one per invocation that
/// woke it.
fn resumes(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| matches!(e.payload(), Some(EventPayload::Run(RunEvent::Resumed(_)))))
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
    let bench = paused_run(Isolation::Worktree).await;
    git(
        &bench.worktree,
        &["commit", "-q", "--allow-empty", "-m", "what the run did"],
    );

    let RunReport { terminal, state } = bench
        .try_wake()
        .await
        .expect("the run wakes and pauses again");

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&bench), 1, "the run woke normally");
    assert!(
        state.effective_findings().is_empty(),
        "a tree that moved forward is the expected case: {:?}",
        state.effective_findings()
    );
}

/// The worktree is the work, so its content is never verified: a person
/// who opens a paused run's checkout and fixes something by hand is using
/// the system as designed, and the run resumes over their edit. What
/// answers a changed tree is the criteria memo, which re-runs the criteria
/// against it rather than reusing a result about a tree that is gone.
#[tokio::test]
async fn a_worktree_edited_by_hand_during_the_pause_resumes_because_the_content_is_the_work() {
    let bench = paused_run(Isolation::Worktree).await;
    std::fs::write(bench.worktree.join("fixed-by-hand.rs"), "fn main() {}").unwrap();
    std::fs::write(bench.worktree.join(".gitkeep"), "touched").unwrap();

    let RunReport { terminal, state } = bench
        .try_wake()
        .await
        .expect("uncommitted work in the tree is not a reason to refuse the run");

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&bench), 1, "the run woke normally");
    assert!(
        state.effective_findings().is_empty(),
        "an edited tree is not reported as anything: {:?}",
        state.effective_findings()
    );
}

#[tokio::test]
async fn a_worktree_reset_behind_the_runs_base_commit_leaves_the_run_broken() {
    let bench = paused_run(Isolation::Worktree).await;
    git(&bench.worktree, &["reset", "--hard", "-q", "HEAD~1"]);
    let head = git_output(&bench.worktree, &["rev-parse", "HEAD"]);

    let error = bench
        .try_wake()
        .await
        .expect_err("the run's history describes a tree this one does not continue");

    let diagnostic = broken(&error);
    assert!(
        diagnostic.contains(&bench.worktree.display().to_string()),
        "the diagnostic names the worktree: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&base_commit(&bench)),
        "the diagnostic names the base commit: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&head),
        "the diagnostic names the HEAD it found: {diagnostic}"
    );
    assert_eq!(
        resumes(&bench),
        0,
        "a run that cannot be verified never records that it resumed"
    );
}

#[tokio::test]
async fn a_worktree_moved_to_an_unrelated_branch_leaves_the_run_broken() {
    let bench = paused_run(Isolation::Worktree).await;
    git(
        &bench.worktree,
        &["checkout", "-q", "--orphan", "somewhere-else"],
    );
    git(
        &bench.worktree,
        &["commit", "-q", "--allow-empty", "-m", "another history"],
    );

    let error = bench
        .try_wake()
        .await
        .expect_err("a tree on an unrelated history is not the run's tree");

    let diagnostic = broken(&error);
    assert!(
        diagnostic.contains(&base_commit(&bench)),
        "the diagnostic names the base commit it looked for: {diagnostic}"
    );
    assert!(
        diagnostic.contains(&run_branch(&bench.run_id)),
        "the remedy names the run's own branch: {diagnostic}"
    );
}

/// A worktree somebody deleted is not a broken run: the run's log and its
/// objects — everything that is its evidence — are untouched, and git
/// still holds its branch, so the checkout comes back with one command.
/// The error says which one.
#[tokio::test]
async fn a_worktree_that_is_gone_is_an_error_that_says_how_to_bring_it_back() {
    let bench = paused_run(Isolation::Worktree).await;
    std::fs::remove_dir_all(&bench.worktree).unwrap();

    let error = bench
        .try_wake()
        .await
        .expect_err("there is no tree at the path the manifest froze");

    let RunError::Worktree(WorktreeError::RunWorktreeLost { .. }) = &error else {
        panic!("a missing worktree is an error with a remedy, not a broken run: {error:?}");
    };
    let message = error.to_string();
    assert!(
        message.contains(&bench.worktree.display().to_string())
            && message.contains(&run_branch(&bench.run_id)),
        "the error names the path and the branch to recreate it from: {message}"
    );
    assert!(
        message.contains("git worktree add"),
        "the error says how to bring the checkout back: {message}"
    );
    assert_eq!(
        resumes(&bench),
        0,
        "a run whose tree cannot be found never records that it resumed"
    );
}

/// `isolation: none` runs on the checkout it was created in rather than on
/// a worktree of its own, and the same two questions apply: the run's
/// derived state describes that checkout's history just as much.
#[tokio::test]
async fn an_isolation_none_checkout_reset_behind_the_base_commit_leaves_the_run_broken() {
    let bench = paused_run(Isolation::None).await;
    git(&bench.worktree, &["reset", "--hard", "-q", "HEAD~1"]);

    let error = bench
        .try_wake()
        .await
        .expect_err("the checkout no longer has the commit the run started from");

    let diagnostic = broken(&error);
    assert!(
        diagnostic.contains(&base_commit(&bench)),
        "the diagnostic names the base commit: {diagnostic}"
    );
    assert!(
        diagnostic.contains("isolation: none"),
        "the remedy is the one for a run that works on your own checkout: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(&run_branch(&bench.run_id)),
        "a run with no worktree of its own has no run branch to send anyone to: {diagnostic}"
    );
}

#[tokio::test]
async fn an_isolation_none_checkout_that_moved_forward_resumes_without_a_word() {
    let bench = paused_run(Isolation::None).await;
    git(
        &bench.worktree,
        &["commit", "-q", "--allow-empty", "-m", "what the run did"],
    );
    std::fs::write(bench.worktree.join("fixed-by-hand.rs"), "fn main() {}").unwrap();

    let RunReport { terminal, .. } = bench
        .try_wake()
        .await
        .expect("a checkout that carries the run's own work forward is the expected case");

    assert!(matches!(terminal, RunTerminal::Paused { .. }));
    assert_eq!(resumes(&bench), 1, "the run woke normally");
}
