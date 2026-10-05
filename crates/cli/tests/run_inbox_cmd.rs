//! `yunta list --runs` inside a repository lists that repository's runs,
//! and says how many runs on the machine belong to other projects;
//! `--all` lists them all.

use yunta_testkit::{handle, run_id_from, runs_root, stderr, stdout, yunta_at, Checkout};

const ONE_NODE: &str = "name: one\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";

/// Two repositories whose runs share one state root, as every run on one
/// machine does, each with one run of its own. Runs in a worktree of its
/// own, so each has its own branch.
fn two_projects(root: &std::path::Path) -> ((Checkout, String), (Checkout, String)) {
    let first = Checkout::under(&root.join("first"))
        .workflow("wf", ONE_NODE)
        .committed();
    let mut second = Checkout::under(&root.join("second"))
        .workflow("wf", ONE_NODE)
        .committed();
    second.home = first.home.clone();
    let ran = |checkout: &Checkout| {
        let run = yunta_at!(checkout, &["run", "wf.yaml"]);
        assert!(run.status.success(), "{}", stderr(&run));
        run_id_from(&run)
    };
    let (first_run, second_run) = (ran(&first), ran(&second));
    ((first, first_run), (second, second_run))
}

#[test]
fn list_runs_shows_only_runs_of_the_repository_it_runs_in() {
    let root = tempfile::tempdir().unwrap();
    let ((first, first_run), (_, second_run)) = two_projects(root.path());

    let listed = stdout(&yunta_at!(&first, &["list", "--runs"]));
    assert!(listed.contains(handle(&first_run)), "{listed}");
    assert!(!listed.contains(handle(&second_run)), "{listed}");
    assert!(
        listed.contains("1 run in other projects — yunta list --runs --all"),
        "{listed}"
    );
}

#[test]
fn list_runs_all_shows_every_run() {
    let root = tempfile::tempdir().unwrap();
    let ((first, first_run), (_, second_run)) = two_projects(root.path());

    let listed = stdout(&yunta_at!(&first, &["list", "--runs", "--all"]));
    assert!(
        listed.contains(handle(&first_run)) && listed.contains(handle(&second_run)),
        "{listed}"
    );
    assert!(!listed.contains("other projects"), "{listed}");
}

#[test]
fn a_run_from_before_projects_were_recorded_belongs_where_its_branch_is() {
    let root = tempfile::tempdir().unwrap();
    let ((first, first_run), (second, _)) = two_projects(root.path());
    // A manifest frozen before runs recorded their repository.
    let manifest = runs_root(&first.home)
        .join(&first_run)
        .join("manifest.yaml");
    let frozen = std::fs::read_to_string(&manifest).unwrap();
    let without: String = frozen
        .lines()
        .scan(false, |inside, line| {
            if line.starts_with("project:") {
                *inside = true;
                return Some(None);
            }
            if *inside && line.starts_with(' ') {
                return Some(None);
            }
            *inside = false;
            Some(Some(line))
        })
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(without, frozen, "the manifest recorded its project");
    std::fs::write(&manifest, without).unwrap();

    let here = stdout(&yunta_at!(&first, &["list", "--runs"]));
    assert!(
        here.contains(handle(&first_run)),
        "its branch is here: {here}"
    );
    let there = stdout(&yunta_at!(&second, &["list", "--runs"]));
    assert!(
        !there.contains(handle(&first_run)),
        "its branch is not there: {there}"
    );
}
