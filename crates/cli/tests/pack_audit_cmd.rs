//! `yunta pack audit` end to end: the real
//! compiled binary against an installed pack, plus `add`'s own
//! automatic audit before vendoring.

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

/// A pack with a distinctive, multi-line prompt and a plain bash node —
/// enough surface to check the audit shows both without trimming.
fn write_pack(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\n\
         nodes:\n\
         \x20 - id: noop\n\
         \x20   kind: bash\n\
         \x20   run: \"true\"\n\
         \x20 - id: brief\n\
         \x20   kind: prompt\n\
         \x20   prompt: |\n\
         \x20     Line one of a distinctive multi-line prompt.\n\
         \x20     Line two, still here, not summarized.\n",
    )
    .unwrap();
    git_ok(dir, &["add", "."]);
    git_ok(dir, &["commit", "-q", "-m", "v1"]);
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack(&upstream);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    std::fs::write(repo.join(".gitkeep"), "").unwrap();
    git_ok(&repo, &["add", "."]);
    git_ok(&repo, &["commit", "-q", "-m", "initial"]);

    let home = root.path().join("state");
    (root, upstream, home)
}

#[test]
fn add_shows_the_full_inventory_before_vendoring() {
    let (root, upstream, home) = setup();
    let repo = root.path().join("repo");

    let out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("pack: acme/review-pack"), "{text}");
    assert!(text.contains("node `noop` (kind: bash)"), "{text}");
    assert!(text.contains("command: true"), "{text}");
    assert!(text.contains("node `brief` (kind: prompt)"), "{text}");
    assert!(
        text.contains("Line one of a distinctive multi-line prompt."),
        "{text}"
    );
    assert!(
        text.contains("Line two, still here, not summarized."),
        "{text}"
    );
    assert!(text.contains("tests: none shipped"), "{text}");
}

#[test]
fn audit_on_demand_reports_the_same_inventory_for_an_installed_pack() {
    let (root, upstream, home) = setup();
    let repo = root.path().join("repo");

    let add_out = yunta_in(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let out = yunta_in(&repo, &home, &["pack", "audit", "acme/review-pack"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("pack: acme/review-pack"), "{text}");
    assert!(
        text.contains("Line two, still here, not summarized."),
        "{text}"
    );
}

#[test]
fn audit_on_an_uninstalled_pack_is_refused() {
    let (root, _upstream, home) = setup();
    let repo = root.path().join("repo");

    let out = yunta_in(&repo, &home, &["pack", "audit", "acme/review-pack"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("isn't installed"), "{}", stderr(&out));
}
