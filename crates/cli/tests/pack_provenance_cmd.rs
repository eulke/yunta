//! End to end: the run manifest freezes which pack (and exactly
//! which version) a top-level workflow came from, and a `pack update`
//! while the run is paused never alters what it does on resume
//! — against the real compiled binary.

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

fn git(dir: &Path, args: &[&str]) -> Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run git")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo(dir: &Path) {
    git_ok(dir, &["init", "-q", "-b", "master"]);
    git_ok(dir, &["config", "user.email", "test@example.com"]);
    git_ok(dir, &["config", "user.name", "Test"]);
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn run_id_from(output: &Output) -> String {
    stdout(output)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .expect("run id in output")
}

/// `lint` fails until `fixed.txt` exists, exhausts its one re-route
/// (`max_reroutes: 0`) and pauses the run — the same proven pattern
/// `run_flow.rs`'s own `resolve-gate` tests use to get a paused run
/// with no interactive gate or forge involved. `fix_lint_run` is the
/// one line that differs between pack versions.
fn write_pack(dir: &Path, version: &str, fix_lint_run: &str) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        format!(
            "name: review-pack\n\
             publisher: acme\n\
             version: {version}\n\
             declares:\n  permissions: full\n  network: false\n  executors: []\n\
             contents:\n  workflows: [workflows/review.yaml]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        format!(
            "name: review\n\
             nodes:\n\
             \x20 - id: lint\n\
             \x20   kind: bash\n\
             \x20   run: \"test -f fixed.txt\"\n\
             \x20   on_failure: {{ goto: fix-lint, max_reroutes: 0 }}\n\
             \x20 - id: fix-lint\n\
             \x20   kind: bash\n\
             \x20   run: \"{fix_lint_run}\"\n"
        ),
    )
    .unwrap();
    git_ok(dir, &["add", "."]);
    git_ok(dir, &["commit", "-q", "-m", version]);
}

#[test]
fn a_pack_update_while_a_run_is_paused_never_changes_what_resume_does() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    // v1's fix succeeds; v2's is a saboteur — if resume ever picked up
    // the updated pack instead of the frozen manifest, the run would
    // fail here instead of finishing.
    write_pack(&upstream, "1.0.0", "touch fixed.txt");

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    std::fs::write(repo.join(".gitkeep"), "").unwrap();
    git_ok(&repo, &["add", "."]);
    git_ok(&repo, &["commit", "-q", "-m", "initial"]);
    let home = root.path().join("state");

    let add_out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let run_out = yunta_in(&repo, &home, &["run", "acme/review"]);
    assert!(
        stdout(&run_out).contains("paused"),
        "expected the exhausted re-route to pause the run: {}\nstderr: {}",
        stdout(&run_out),
        stderr(&run_out)
    );
    let run_id = run_id_from(&run_out);

    // The manifest already names the pack and the
    // exact version it was resolved against at creation time.
    let manifest_path = home.join("runs").join(&run_id).join("manifest.yaml");
    let manifest_text = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["pack"]["publisher"], "acme");
    assert_eq!(manifest["pack"]["name"], "review-pack");
    assert_eq!(manifest["pack"]["version"], "1.0.0");
    assert!(
        manifest["pack"]["commit"].as_str().unwrap().len() >= 7,
        "{manifest:?}"
    );

    // Now the pack updates to v2 — different behavior, same identity —
    // while the run is still sitting paused.
    write_pack(&upstream, "2.0.0", "false");
    let update_out = yunta_in(
        &repo,
        &home,
        &["pack", "update", "acme/review-pack", "master"],
    );
    assert!(update_out.status.success(), "{}", stderr(&update_out));
    let vendored_after_update =
        std::fs::read_to_string(repo.join(".yunta/packs/acme/review-pack/pack.yaml")).unwrap();
    assert!(vendored_after_update.contains("2.0.0"));

    // The manifest on disk must be exactly what it was before the
    // update — resume never re-reads the pack.
    let manifest_text_after_update = std::fs::read_to_string(&manifest_path).unwrap();
    assert_eq!(manifest_text, manifest_text_after_update);

    let resolve = yunta_in(&repo, &home, &["resolve-gate", &run_id, "retry"]);
    assert!(resolve.status.success(), "{}", stderr(&resolve));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = yunta_in(&repo, &home, &["status", &run_id]);
        let text = stdout(&status);
        if text.contains("finished") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached finished after resolve-gate: {text}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
