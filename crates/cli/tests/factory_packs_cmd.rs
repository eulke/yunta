//! `yunta/starter` and `yunta/fragua` installed
//! and checked through the real pack pipeline — `pack add`, `check`,
//! and `pack audit`'s own test discovery — against the actual pack
//! directories this repo ships at `packs/starter`/`packs/fragua`, not a
//! synthetic stand-in.

use std::path::Path;

use yunta_testkit::{git, stderr, stdout, yunta_in};

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Copies a pack directory into a fresh git repo — `pack add` clones by
/// ref, so the source needs to be a real repository, not the pack's own
/// (uncommitted-as-a-repo) place inside this monorepo.
fn git_ify(pack_dir: &Path) -> tempfile::TempDir {
    let staged = tempfile::tempdir().unwrap();
    copy_dir(pack_dir, staged.path());
    git(staged.path(), &["init", "-q", "-b", "master"]);
    git(staged.path(), &["config", "user.email", "test@example.com"]);
    git(staged.path(), &["config", "user.name", "Test"]);
    git(staged.path(), &["add", "."]);
    git(staged.path(), &["commit", "-q", "-m", "snapshot"]);
    staged
}

fn copy_dir(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dest = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

fn setup_project() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "master"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "runners:\n  executor:\n    - { adapter: mock, model: mock-model }\n  reviewer:\n    - { adapter: mock, model: mock-model }\n  reviewer-alt:\n    - { adapter: mock, model: mock-model }\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "initial"]);
    let home = root.path().join("state");
    (root, repo, home)
}

#[test]
fn yunta_starter_installs_checks_and_self_tests_through_the_real_pack_pipeline() {
    let pack_source = git_ify(&repo_root().join("packs/starter"));
    let (_root, repo, home) = setup_project();

    let add_out = yunta_in!(
        &repo,
        &home,
        &[
            "pack",
            "add",
            "--run-tests",
            pack_source.path().to_str().unwrap(),
        ]
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));
    let add_text = stdout(&add_out);
    assert!(
        add_text.contains("tests: 2 case(s), 0 failed"),
        "{add_text}"
    );

    let check_fix = yunta_in!(&repo, &home, &["check", "yunta/fix"]);
    assert!(check_fix.status.success(), "{}", stderr(&check_fix));
    let check_review = yunta_in!(&repo, &home, &["check", "yunta/review"]);
    assert!(check_review.status.success(), "{}", stderr(&check_review));

    let audit = yunta_in!(&repo, &home, &["pack", "audit", "yunta/starter"]);
    assert!(audit.status.success(), "{}", stderr(&audit));
    assert!(stdout(&audit).contains("tests: 2 case(s), 0 failed"));
}

#[test]
fn yunta_fragua_installs_and_checks_every_declared_mode_through_the_real_pack_pipeline() {
    let pack_source = git_ify(&repo_root().join("packs/fragua"));
    let (_root, repo, home) = setup_project();
    // fragua's own runners, beyond the review-fanout pair setup() already
    // configures.
    let config_path = repo.join(".yunta/config.yaml");
    std::fs::write(
        &config_path,
        "runners:\n  planner:\n    - { adapter: mock, model: mock-model }\n  executor:\n    - { adapter: mock, model: mock-model }\n  mechanical:\n    - { adapter: mock, model: mock-model }\n  reviewer:\n    - { adapter: mock, model: mock-model }\n  reviewer-alt:\n    - { adapter: mock, model: mock-model }\nproject:\n  base_branch: master\nbaseline:\n  suite: \"true\"\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fragua runners"]);

    let add_out = yunta_in!(
        &repo,
        &home,
        &[
            "pack",
            "add",
            "--run-tests",
            pack_source.path().to_str().unwrap(),
        ]
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));
    // fragua ships one case per declared mode; `--run-tests` runs them
    // against the mock once the pack is installed.
    assert!(
        stdout(&add_out).contains("tests: 3 case(s), 0 failed"),
        "{}",
        stdout(&add_out)
    );

    // `check` validates mode-coherence for every declared mode in one
    // pass — this is real schema/reference validation against
    // the actual installed file, not a hand-copied stand-in.
    let check = yunta_in!(&repo, &home, &["check", "yunta/build-feature"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(stdout(&check).contains("OK"));
}

#[test]
fn test_dir_runs_a_packs_own_cases_from_outside_its_root() {
    let (_root, repo, home) = setup_project();
    let pack = repo_root().join("packs/starter");

    let out = yunta_in!(&repo, &home, &["test", "--dir", pack.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("2 case(s), 0 failed"), "{text}");
}

/// Uninstalling either pack leaves the engine's own capabilities
/// untouched — checked by structural test in
/// `crates/cli/tests/factory_packs_structural.rs` (no crate source
/// references these packs by name); this one checks the CLI-visible
/// half of the same claim: nothing else in the project breaks once the
/// pack is gone.
#[test]
fn removing_a_factory_pack_leaves_the_rest_of_the_project_working() {
    let pack_source = git_ify(&repo_root().join("packs/starter"));
    let (_root, repo, home) = setup_project();
    std::fs::create_dir_all(repo.join(".yunta/workflows")).unwrap();
    std::fs::write(
        repo.join(".yunta/workflows/local.yaml"),
        "name: local\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();

    let add_out = yunta_in!(
        &repo,
        &home,
        &[
            "pack",
            "add",
            "--run-tests",
            pack_source.path().to_str().unwrap(),
        ]
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let remove_out = yunta_in!(&repo, &home, &["pack", "remove", "yunta/starter"]);
    assert!(remove_out.status.success(), "{}", stderr(&remove_out));
    assert!(!repo.join(".yunta/packs/yunta/starter").exists());

    let check = yunta_in!(&repo, &home, &["check", "local"]);
    assert!(check.status.success(), "{}", stderr(&check));
}
