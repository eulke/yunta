//! Isolation on a real repository: the checkout a run works in and the
//! lock that says it is a run's.
//!
//! `worktree` gives each run its own checkout at the base commit;
//! `none` takes the repository itself and refuses a dirty tree. A lock
//! is taken from a dead owner and reported, never from a live one, and
//! never from an owner this process cannot verify — a reused pid is not
//! the owner, and an unreadable record refuses rather than guesses.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::process::signal::Liveness;
use yunta_core::{CommitSha, Isolation, Pid, SystemClock};
use yunta_engine::lock::{acquire, Acquired, Contention, LockError, LockOwner, OwnerProbe};
use yunta_engine::{
    commit_work, land, open_unit, prepare_worktree, rebase_onto, release_worktree, run_branch,
    unit_branch, Rebase, Unit, UnitHome, UnitId, WorktreeError,
};
use yunta_testkit::{git_output, init_repo, Owner};

fn head(dir: &Path) -> CommitSha {
    git_output(dir, &["rev-parse", "HEAD"])
        .trim()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn worktree_isolation_creates_a_real_git_worktree_at_base_commit() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    let worktree_path = root.path().join("worktrees/run-1");

    prepare_worktree(
        &repo,
        &worktree_path,
        &base_commit,
        "yunta/run-1",
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .unwrap();

    assert!(worktree_path.join(".gitkeep").exists());
    assert_eq!(head(&worktree_path), base_commit);
    // It's a real worktree of the same repo, not a detached clone.
    let common_dir = git_output(&worktree_path, &["rev-parse", "--git-common-dir"]);
    assert!(common_dir.starts_with(repo.join(".git").to_str().unwrap()));
}

#[tokio::test]
async fn two_worktree_isolated_runs_on_the_same_repo_never_collide() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    let wt1 = root.path().join("worktrees/run-1");
    let wt2 = root.path().join("worktrees/run-2");
    prepare_worktree(
        &repo,
        &wt1,
        &base_commit,
        "yunta/run-1",
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .unwrap();
    prepare_worktree(
        &repo,
        &wt2,
        &base_commit,
        "yunta/run-2",
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .unwrap();

    std::fs::write(wt1.join("only-in-1.txt"), "one").unwrap();
    std::fs::write(wt2.join("only-in-2.txt"), "two").unwrap();

    assert!(!wt1.join("only-in-2.txt").exists());
    assert!(!wt2.join("only-in-1.txt").exists());
    assert!(!repo.join("only-in-1.txt").exists());
    assert!(!repo.join("only-in-2.txt").exists());
}

#[tokio::test]
async fn none_isolation_with_a_clean_tree_succeeds_and_locks_the_repo() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();

    // A second run on the same repo must be refused while the first
    // holds the lock — this is the "no concurrent runs" guarantee
    // required for `none`.
    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, WorktreeError::Locked { .. }));
}

#[tokio::test]
async fn none_isolation_with_a_dirty_tree_is_refused_before_anything_runs() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    std::fs::write(repo.join("uncommitted.txt"), "dirty").unwrap();

    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, WorktreeError::DirtyTree { .. }));
}

#[tokio::test]
async fn releasing_a_none_isolation_lock_lets_a_later_run_proceed() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
    release_worktree(&repo, Isolation::None, owner.supervision())
        .await
        .unwrap();

    // No longer locked — a fresh run may proceed.
    prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn releasing_a_worktree_isolated_run_leaves_the_worktree_on_disk() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    let worktree_path = root.path().join("worktrees/run-1");

    prepare_worktree(
        &repo,
        &worktree_path,
        &base_commit,
        "yunta/run-1",
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .unwrap();
    release_worktree(&repo, Isolation::Worktree, owner.supervision())
        .await
        .unwrap();

    // Worktrees are left in place for inspection — cleanup is a
    // separate, not-yet-built concern (on_finish).
    assert!(worktree_path.join(".gitkeep").exists());
}

// --- owner-aware `none` lock ------------------------------------------

fn lock_file(repo: &std::path::Path) -> std::path::PathBuf {
    repo.join(".git/yunta-none.lock")
}

fn mutation_lock_file(repo: &std::path::Path) -> std::path::PathBuf {
    repo.join(".git/yunta-worktree.lock")
}

/// The record every lock writer produces: the holder's pid and when it
/// took the lock.
fn owner_record(pid: u32, started_at: DateTime<Utc>) -> String {
    serde_json::json!({ "pid": pid, "started_at": started_at }).to_string()
}

/// A pid that is certainly dead: a child spawned and reaped here.
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

/// A probe that answers what the test decides, whatever the pid.
struct Scripted {
    liveness: Liveness,
    started: Option<DateTime<Utc>>,
}

impl OwnerProbe for Scripted {
    fn liveness(&self, _pid: Pid) -> Liveness {
        self.liveness
    }

    fn started(&self, _pid: Pid) -> Option<DateTime<Utc>> {
        self.started
    }
}

fn read_owner(path: &Path) -> LockOwner {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
async fn a_dead_owner_s_lock_is_stolen_and_the_takeover_is_reported() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    // A pid that is certainly dead: a child we spawn and reap ourselves.
    let dead = std::process::Command::new("true").spawn().unwrap();
    let dead_pid = dead.id();
    let _ = std::process::Child::wait(&mut { dead });
    std::fs::write(lock_file(&repo), owner_record(dead_pid, Utc::now())).unwrap();

    let prepared = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
    match prepared {
        yunta_engine::WorktreePrepared::StoleStaleLock { dead_pid: reported } => {
            assert_eq!(reported.as_u32(), dead_pid);
        }
        other => panic!("expected the stale lock to be stolen, got {other:?}"),
    }
    // The lock now names this process as its owner.
    let content = std::fs::read_to_string(lock_file(&repo)).unwrap();
    assert!(
        content.contains(&std::process::id().to_string()),
        "got: {content}"
    );
}

#[tokio::test]
async fn a_live_owner_s_lock_still_refuses() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    // This test process itself is the live owner.
    std::fs::write(
        lock_file(&repo),
        owner_record(std::process::id(), Utc::now()),
    )
    .unwrap();

    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, WorktreeError::Locked { .. }), "got: {err:?}");
}

#[tokio::test]
async fn a_legacy_empty_lock_refuses_conservatively_naming_the_file() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    std::fs::write(lock_file(&repo), "").unwrap();

    let err = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("yunta-none.lock"),
        "the refusal must name the file to delete by hand: {message}"
    );
    assert!(
        !matches!(err, WorktreeError::Locked { .. }),
        "an unverifiable owner is its own case, not a live-owner refusal"
    );
}

// --- concurrent `git worktree` mutations never corrupt metadata -------

/// Git mutates `.git/worktrees/` without a complete lock between `add`s
/// — N concurrent additions on one repo can read each other's
/// half-written metadata (`failed to read .git/worktrees/<x>/commondir`).
/// Exactly what a `concurrency: N` task batch, two `kind: workflow`
/// nodes in one batch, or two MCP `run_workflow` calls do. The engine's
/// own lock around every worktree mutation is what makes this pass
/// deterministically.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_worktree_adds_on_one_repo_never_corrupt_git_metadata() {
    let owner = Owner::new();
    // `Supervision` is a copy of two borrows, so every task that spawns
    // in this round takes its own without moving what they point at.
    let supervision = owner.supervision();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    for round in 0..3 {
        let adds = (0..8).map(|i| {
            let repo = repo.clone();
            let base_commit = base_commit.clone();
            let path = root.path().join(format!("worktrees/r{round}-w{i}"));
            let branch = format!("yunta/stress/r{round}-w{i}");
            async move {
                prepare_worktree(
                    &repo,
                    &path,
                    &base_commit,
                    &branch,
                    Isolation::Worktree,
                    supervision,
                )
                .await
            }
        });
        let results = futures::future::join_all(adds).await;
        for (i, result) in results.into_iter().enumerate() {
            result.unwrap_or_else(|e| panic!("round {round}, add {i} failed: {e}"));
        }
    }
}

// --- one lock protocol, honest about what it cannot tell --------------

#[tokio::test]
async fn unknown_liveness_never_steals_a_lock() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    let taken = Utc::now();
    std::fs::write(&lock_path, owner_record(4242, taken)).unwrap();
    let probe = Scripted {
        liveness: Liveness::Unknown,
        started: None,
    };

    // The `none` lock refuses at once, naming the holder it could not
    // verify...
    let err = acquire(&lock_path, Contention::Refuse, &probe, &SystemClock)
        .await
        .unwrap_err();
    match err {
        LockError::Held {
            owner, liveness, ..
        } => {
            assert_eq!(owner.pid.as_u32(), 4242);
            assert_eq!(liveness, Liveness::Unknown);
        }
        other => panic!("expected the lock to be held, got {other:?}"),
    }

    // ...and the mutation lock waits its patience out, then gives up —
    // it never steals from a holder it cannot ask.
    let err = acquire(
        &lock_path,
        Contention::Wait {
            patience: Duration::from_millis(60),
            poll: Duration::from_millis(10),
        },
        &probe,
        &SystemClock,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LockError::Timeout { .. }), "got: {err:?}");
    assert_eq!(
        std::fs::read_to_string(&lock_path).unwrap(),
        owner_record(4242, taken),
        "the file still names the holder that could not be verified"
    );
}

#[tokio::test]
async fn a_reused_pid_is_not_the_owner() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    let taken = Utc::now();
    std::fs::write(&lock_path, owner_record(4242, taken)).unwrap();

    // Alive, but started after the lock was taken: a newcomer that got
    // the old owner's pid.
    let newcomer = Scripted {
        liveness: Liveness::Alive,
        started: Some(taken + chrono::Duration::minutes(1)),
    };
    let acquired = acquire(&lock_path, Contention::Refuse, &newcomer, &SystemClock)
        .await
        .unwrap();
    assert!(
        matches!(acquired, Acquired::Stolen { ref dead } if dead.pid.as_u32() == 4242),
        "got: {acquired:?}"
    );
    assert_eq!(read_owner(&lock_path).pid, Pid::current());
}

#[tokio::test]
async fn a_live_holder_with_an_unknown_start_time_keeps_its_lock() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("yunta.lock");
    std::fs::write(&lock_path, owner_record(4242, Utc::now())).unwrap();

    // A host that cannot tell when a process started: liveness alone
    // decides, and alive means held.
    let probe = Scripted {
        liveness: Liveness::Alive,
        started: None,
    };
    let err = acquire(&lock_path, Contention::Refuse, &probe, &SystemClock)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            LockError::Held {
                liveness: Liveness::Alive,
                ..
            }
        ),
        "got: {err:?}"
    );
}

#[tokio::test]
async fn both_locks_share_one_protocol() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    // The same stale record on both locks: a dead holder that took them
    // an hour ago.
    let stale = owner_record(dead_pid(), Utc::now() - chrono::Duration::hours(1));
    std::fs::write(lock_file(&repo), &stale).unwrap();
    std::fs::write(mutation_lock_file(&repo), &stale).unwrap();
    // A hard link to the holder's file: it keeps the stale record if the
    // steal replaces the file, and shows the new one if the steal wrote
    // over it.
    let witness = root.path().join("witness");
    std::fs::hard_link(lock_file(&repo), &witness).unwrap();

    // `isolation: none`: stolen by remove + create_new — a new file, a
    // fresh owner record — and reported.
    let prepared = prepare_worktree(
        &repo,
        &repo,
        &base_commit,
        "unused",
        Isolation::None,
        owner.supervision(),
    )
    .await
    .unwrap();
    assert!(
        matches!(
            prepared,
            yunta_engine::WorktreePrepared::StoleStaleLock { .. }
        ),
        "got: {prepared:?}"
    );
    let holder = read_owner(&lock_file(&repo));
    assert_eq!(holder.pid, Pid::current());
    assert!(holder.started_at > Utc::now() - chrono::Duration::minutes(1));
    assert_eq!(
        std::fs::read_to_string(&witness).unwrap(),
        stale,
        "a steal replaces the file; it never writes over the holder's file"
    );

    // The worktree-mutation lock: the same dead holder is stolen the
    // same way, the mutation runs, and the lock is released after it.
    let worktree = root.path().join("wt");
    prepare_worktree(
        &repo,
        &worktree,
        &base_commit,
        "yunta/shared-protocol",
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .unwrap();
    assert!(worktree.join(".gitkeep").exists());
    assert!(
        !mutation_lock_file(&repo).exists(),
        "the mutation lock is released once the mutation is done"
    );
}

/// A run's own branch and the branches of its units' worktrees share a
/// repository's ref namespace, and git refuses a ref that is a directory
/// of another: `refs/heads/a` and `refs/heads/a/b` cannot both exist. The
/// two shapes have to be siblings, whatever a run is called.
#[tokio::test]
async fn a_run_branch_and_its_unit_branches_coexist() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    let run = yunta_core::RunId::from("01JBRANCHCOEXISTENCE0000AB");

    prepare_worktree(
        &repo,
        &root.path().join("trees/run"),
        &base_commit,
        &run_branch(&run),
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .expect("the run's own branch");
    prepare_worktree(
        &repo,
        &root.path().join("trees/task"),
        &base_commit,
        &unit_branch(&run, &UnitId::Task("T001".into()), 1),
        Isolation::Worktree,
        owner.supervision(),
    )
    .await
    .expect("a unit branch of the same run, beside it and not under it");
}

/// A unit branch names the run that made it: the worktree it belongs to
/// is the run's, but a ref belongs to the whole repository, so two runs
/// working the same unit id in one checkout would otherwise ask git for
/// the same branch — and the second one fails.
#[tokio::test]
async fn two_runs_working_the_same_unit_get_their_own_branches() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);
    let first = yunta_core::RunId::from("01JBRANCHFIRSTRUN00000000A");
    let second = yunta_core::RunId::from("01JBRANCHSECONDRUN0000000B");
    let task = UnitId::Task("T001".into());

    assert_ne!(
        unit_branch(&first, &task, 1),
        unit_branch(&second, &task, 1)
    );
    for (run, tree) in [(&first, "trees/first"), (&second, "trees/second")] {
        prepare_worktree(
            &repo,
            &root.path().join(tree),
            &base_commit,
            &unit_branch(run, &task, 1),
            Isolation::Worktree,
            owner.supervision(),
        )
        .await
        .expect("each run's own unit branch");
    }
}

/// A repository whose tree and whose unit both changed one file, which
/// is what a replay cannot reconcile — the setup both conflict cases
/// need, and nothing either of them asserts on.
async fn a_unit_at_odds_with_its_tree(root: &Path, run: &str) -> (std::path::PathBuf, Unit) {
    let owner = Owner::new();
    let repo = root.join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    tokio::fs::write(repo.join("shared.txt"), "base\n")
        .await
        .unwrap();
    yunta_testkit::git(&repo, &["add", "-A"]);
    yunta_testkit::git(&repo, &["commit", "-q", "-m", "shared"]);

    let unit = open_unit(
        UnitHome {
            repo: &repo,
            run_dir: root,
            run_id: &yunta_core::RunId::from(run),
            base: &head(&repo),
        },
        UnitId::Task("T001".into()),
        1,
        owner.supervision(),
    )
    .await
    .expect("the unit opens in a tree of its own");

    tokio::fs::write(unit.worktree.join("shared.txt"), "the unit's\n")
        .await
        .unwrap();
    commit_work(&unit, "the unit's work", owner.supervision())
        .await
        .expect("the unit commits what it did");
    tokio::fs::write(repo.join("shared.txt"), "somebody else's\n")
        .await
        .unwrap();
    yunta_testkit::git(&repo, &["commit", "-qam", "somebody else's"]);
    (repo, unit)
}

/// A unit's work is replayed onto the tree as it stands when the unit
/// lands, and git may not be able to replay it. What comes back names
/// the paths it could not reconcile: the landing is refused for reasons
/// a person can act on, not for an exit code.
#[tokio::test]
async fn a_landing_that_conflicts_reports_its_paths() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let (repo, unit) =
        a_unit_at_odds_with_its_tree(root.path(), "01JUNITLANDCONFLICT000000A").await;

    match rebase_onto(&unit, &repo, owner.supervision())
        .await
        .expect("git answers, one way or the other")
    {
        Rebase::Conflicts(paths) => assert_eq!(paths, vec![std::path::PathBuf::from("shared.txt")]),
        Rebase::Onto(tree) => panic!("these two cannot both apply, and git said they do: {tree}"),
    }
}

/// And the unit's own tree is left where it was, so the refusal costs
/// nothing: a rebase git could not finish is undone, never left half
/// applied for the next caller to find.
#[tokio::test]
async fn a_unit_whose_landing_conflicts_keeps_its_own_work() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let (repo, unit) =
        a_unit_at_odds_with_its_tree(root.path(), "01JUNITKEEPSITSWORK00000AB").await;

    let _ = rebase_onto(&unit, &repo, owner.supervision()).await;

    assert_eq!(
        tokio::fs::read_to_string(unit.worktree.join("shared.txt"))
            .await
            .unwrap(),
        "the unit's\n",
        "an aborted rebase leaves the unit's own tree exactly as it was"
    );
    assert!(
        git_output(&unit.worktree, &["status", "--porcelain"])
            .trim()
            .is_empty(),
        "and with nothing half-applied in it"
    );
}

/// A unit that lands moves the shared tree onto its work, and says
/// where that tree now stands.
#[tokio::test]
async fn a_unit_that_lands_moves_the_tree_it_landed_in() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    let run = yunta_core::RunId::from("01JUNITLANDSCLEAN00000000A");

    let unit = open_unit(
        UnitHome {
            repo: &repo,
            run_dir: root.path(),
            run_id: &run,
            base: &head(&repo),
        },
        UnitId::Node("build".into()),
        1,
        owner.supervision(),
    )
    .await
    .expect("the unit opens in a tree of its own");
    tokio::fs::write(unit.worktree.join("mine.txt"), "work\n")
        .await
        .unwrap();
    commit_work(&unit, "the unit's work", owner.supervision())
        .await
        .expect("the unit commits what it did");

    assert!(matches!(
        rebase_onto(&unit, &repo, owner.supervision())
            .await
            .unwrap(),
        Rebase::Onto(_)
    ));
    let landed = land(&unit, &repo, owner.supervision())
        .await
        .expect("nothing stands between the tree and the unit's work");

    assert_eq!(head(&repo), landed);
    assert_eq!(
        tokio::fs::read_to_string(repo.join("mine.txt"))
            .await
            .unwrap(),
        "work\n"
    );
}

/// A unit that changed nothing is not an error and produces no commit:
/// criteria satisfied by side effects that left no diff are still met.
#[tokio::test]
async fn a_unit_that_changed_nothing_commits_nothing() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    let base_commit = head(&repo);

    let unit = open_unit(
        UnitHome {
            repo: &repo,
            run_dir: root.path(),
            run_id: &yunta_core::RunId::from("01JUNITCHANGEDNOTHING0000A"),
            base: &base_commit,
        },
        UnitId::Task("T001".into()),
        1,
        owner.supervision(),
    )
    .await
    .expect("the unit opens in a tree of its own");

    commit_work(&unit, "nothing at all", owner.supervision())
        .await
        .expect("a unit that did nothing is not a failure");

    assert_eq!(
        git_output(&unit.worktree, &["rev-parse", "HEAD"]).trim(),
        base_commit.as_str(),
        "nothing staged, nothing committed"
    );
}
