//! A unit's own tree: what it commits, where it lands, and what a rebase
//! refused by git looks like from the unit's side.

use std::path::Path;

use tokio_util::sync::CancellationToken;
use yunta_core::protected::Pathspecs;
use yunta_engine::git::Cause;
use yunta_engine::process::Supervision;
use yunta_engine::{
    carry_work, commit_work, land, rebase_onto, Lease, Rebase, Unit, UnitHome, UnitId,
    WorktreeError,
};
use yunta_testkit::stubs::GIT_REAL;
use yunta_testkit::{git_output, init_repo, Owner};
use yunta_testkit_core::FixedClock;

use super::{head, pool_of};

/// A unit works on no branch: what it commits is kept by its ref, which
/// no list of the repository's branches shows.
#[tokio::test]
async fn a_unit_works_detached_and_its_commits_are_kept_by_its_ref() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let run = yunta_core::RunId::from("01JUNITKEPTBYITSREF0000000");
    let pool = pool_of(&root.path().join("worktrees"), &repo, root.path());
    let home = UnitHome {
        repo: &repo,
        run_dir: root.path(),
        run_id: &run,
        base: &head(&repo),
    };
    let task = UnitId::Task("T001".into());
    let (unit, _held) = pool.open(home, task, 1, owner.supervision()).await.unwrap();

    tokio::fs::write(unit.worktree.join("made.txt"), "made\n")
        .await
        .unwrap();
    commit_work(&unit, "the unit's work", owner.supervision())
        .await
        .unwrap();

    let on = git_output(&unit.worktree, &["branch", "--show-current"]);
    let kept = git_output(&repo, &["rev-parse", unit.keeps.name()]);
    let branches = git_output(&repo, &["branch", "--list", "yunta/unit/*"]);
    assert_eq!(on.trim(), "", "the unit works on no branch");
    assert_eq!(
        kept.trim(),
        git_output(&unit.worktree, &["rev-parse", "HEAD"]).trim()
    );
    assert_eq!(branches.trim(), "");
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

    let (unit, _held) = pool_of(&root.join("worktrees"), &repo, root)
        .open(
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

/// A rebase that git refuses for a dirty tree is an error carrying git's own
/// cause, not a conflict that names no paths.
#[tokio::test]
async fn a_rebase_refused_for_a_dirty_tree_is_an_error_not_a_conflict() {
    let root = tempfile::tempdir().unwrap();
    let (repo, unit) = a_unit_at_odds_with_its_tree(root.path(), "01JUNITDIRTYTREE00000000A").await;
    tokio::fs::write(unit.worktree.join("shared.txt"), "left uncommitted\n")
        .await
        .unwrap();

    let result = rebase_onto(&unit, &repo, Owner::new().supervision()).await;

    assert!(
        matches!(result, Err(WorktreeError::Git(ref e)) if matches!(e.cause, Cause::Exit { code: Some(1), .. })),
        "got {result:?}"
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

    let (unit, _held) = pool_of(&root.path().join("worktrees"), &repo, root.path())
        .open(
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

    let (unit, _held) = pool_of(&root.path().join("worktrees"), &repo, root.path())
        .open(
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

/// The environment of a `git` that refuses every `cherry-pick` with exit 1
/// and no path unmerged, the shape git gives a refusal that is not a
/// conflict. Everything else runs the real git the stub names.
fn git_refusing_picks(dir: &Path) -> Vec<(String, String)> {
    // The testkit's stub supplies the environment: its `PATH` puts `bin`
    // first, and its variables name the real git. Its script is replaced.
    let env = yunta_testkit::stubs::git(dir);
    let script = format!(
        r#"#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = cherry-pick ]; then
    echo refused >&2
    exit 1
  fi
done
exec "${GIT_REAL}" "$@"
"#
    );
    std::fs::write(dir.join("bin/git"), script).unwrap();
    env
}

/// Opens a unit of its own in `repo`, on the commit `repo` has now.
async fn open_a_unit(root: &Path, repo: &Path, run: &str, owner: &Owner) -> (Unit, Lease) {
    pool_of(&root.join("worktrees"), repo, root)
        .open(
            UnitHome {
                repo,
                run_dir: root,
                run_id: &yunta_core::RunId::from(run),
                base: &head(repo),
            },
            UnitId::Task("T001".into()),
            1,
            owner.supervision(),
        )
        .await
        .expect("the unit opens in a tree of its own")
}

/// A pick git refuses without leaving any path unmerged is an error, and
/// the checkout keeps the uncommitted work it had.
#[tokio::test]
async fn a_refused_pick_is_an_error_and_leaves_the_checkout_as_it_was() {
    let owner = Owner::new();
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    tokio::fs::create_dir_all(&repo).await.unwrap();
    init_repo(&repo);
    let (unit, _held) = open_a_unit(root.path(), &repo, "01JUNITREFUSEDPICK0000000", &owner).await;
    tokio::fs::write(unit.worktree.join("carried.txt"), "carried\n")
        .await
        .unwrap();
    commit_work(&unit, "the work a blocked task left", owner.supervision())
        .await
        .expect("the unit commits what it did");
    let left = head(&unit.worktree);
    yunta_testkit::git(
        &unit.worktree,
        &["reset", "-q", "--hard", unit.base.as_str()],
    );
    tokio::fs::write(unit.worktree.join("draft.txt"), "uncommitted\n")
        .await
        .unwrap();

    let env = git_refusing_picks(root.path());
    let cancel = CancellationToken::new();
    let supervision = Supervision {
        env: &env,
        ..Supervision::outside_any_run(&cancel, &FixedClock)
    };
    let result = carry_work(&unit, &left, &Pathspecs::default(), supervision).await;

    assert!(
        matches!(result, Err(WorktreeError::Git(ref e)) if matches!(e.cause, Cause::Exit { code: Some(1), .. })),
        "got {result:?}"
    );
    assert_eq!(
        tokio::fs::read_to_string(unit.worktree.join("draft.txt"))
            .await
            .unwrap(),
        "uncommitted\n",
        "a refused pick takes no uncommitted work with it"
    );
    assert_eq!(
        head(&unit.worktree),
        unit.base,
        "the checkout stays where it was"
    );
}
