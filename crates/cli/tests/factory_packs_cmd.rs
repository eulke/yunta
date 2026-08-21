//! `yunta/starter` and `yunta/fragua` (T10.2, RFC-0001 §3/D57) installed
//! and checked through the real pack pipeline — `pack add`, `check`,
//! and `pack audit`'s own test discovery — against the actual pack
//! directories this repo ships at `packs/starter`/`packs/fragua`, not a
//! synthetic stand-in.

use std::path::Path;
use std::process::Output;

fn yunta_in(dir: &Path, home: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .output()
        .expect("failed to run the yunta binary")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

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
    git_ok(staged.path(), &["init", "-q", "-b", "master"]);
    git_ok(staged.path(), &["config", "user.email", "test@example.com"]);
    git_ok(staged.path(), &["config", "user.name", "Test"]);
    git_ok(staged.path(), &["add", "."]);
    git_ok(staged.path(), &["commit", "-q", "-m", "snapshot"]);
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
    git_ok(&repo, &["init", "-q", "-b", "master"]);
    git_ok(&repo, &["config", "user.email", "test@example.com"]);
    git_ok(&repo, &["config", "user.name", "Test"]);
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "runners:\n  executor:\n    - { adapter: mock, model: mock-model }\n  reviewer:\n    - { adapter: mock, model: mock-model }\n  reviewer-alt:\n    - { adapter: mock, model: mock-model }\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git_ok(&repo, &["add", "."]);
    git_ok(&repo, &["commit", "-q", "-m", "initial"]);
    let home = root.path().join("state");
    (root, repo, home)
}

#[test]
fn yunta_starter_installs_checks_and_self_tests_through_the_real_pack_pipeline() {
    let pack_source = git_ify(&repo_root().join("packs/starter"));
    let (_root, repo, home) = setup_project();

    let add_out = yunta_in(
        &repo,
        &home,
        &["pack", "add", pack_source.path().to_str().unwrap()],
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));
    let add_text = stdout(&add_out);
    assert!(
        add_text.contains("tests: 2 case(s), 0 failed"),
        "{add_text}"
    );

    let check_fix = yunta_in(&repo, &home, &["check", "yunta/fix"]);
    assert!(check_fix.status.success(), "{}", stderr(&check_fix));
    let check_review = yunta_in(&repo, &home, &["check", "yunta/review"]);
    assert!(check_review.status.success(), "{}", stderr(&check_review));

    let audit = yunta_in(&repo, &home, &["pack", "audit", "yunta/starter"]);
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
    git_ok(&repo, &["add", "."]);
    git_ok(&repo, &["commit", "-q", "-m", "fragua runners"]);

    let add_out = yunta_in(
        &repo,
        &home,
        &["pack", "add", pack_source.path().to_str().unwrap()],
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));
    // fragua's own end-to-end mock proof lives in
    // crates/engine/tests/factory_packs.rs — its `.yunta/tests/` case
    // format has no `mode:` field yet (M-0 cut), so a case here could
    // only assert "paused at approve-plan", strictly less than what
    // that engine-level test already proves.
    assert!(stdout(&add_out).contains("tests: none shipped"));

    // `check` validates mode-coherence for every declared mode in one
    // pass (T1.3) — this is real schema/reference validation against
    // the actual installed file, not a hand-copied stand-in.
    let check = yunta_in(&repo, &home, &["check", "yunta/build-feature"]);
    assert!(check.status.success(), "{}", stderr(&check));
    assert!(stdout(&check).contains("OK"));
}

/// D57: uninstalling either pack leaves the engine's own capabilities
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

    let add_out = yunta_in(
        &repo,
        &home,
        &["pack", "add", pack_source.path().to_str().unwrap()],
    );
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let remove_out = yunta_in(&repo, &home, &["pack", "remove", "yunta/starter"]);
    assert!(remove_out.status.success(), "{}", stderr(&remove_out));
    assert!(!repo.join(".yunta/packs/yunta/starter").exists());

    let check = yunta_in(&repo, &home, &["check", "local"]);
    assert!(check.status.success(), "{}", stderr(&check));
}
