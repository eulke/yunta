//! A hermetic git repository fixture.

use std::path::Path;
use std::process::Command;

/// The branch [`init_repo`] starts a repository on. Named explicitly so a
/// test that asserts on a recorded ref reads this constant instead of
/// hard-coding a branch the host might not use by default.
pub const INITIAL_BRANCH: &str = "main";

/// Runs `git` in `dir` and asserts it succeeded, surfacing stderr in the
/// failure message. Global and system git config are pinned to `/dev/null`
/// so a run is identical on every machine — a developer's `~/.gitconfig`
/// (a different `init.defaultBranch`, a hook, a commit template) can never
/// change what a test sees.
pub fn git(dir: &Path, args: &[&str]) {
    let output = git_command(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Like [`git`], but returns the command's trimmed stdout — for the
/// queries a test reads a value from, e.g. `git rev-parse HEAD`.
pub fn git_output(dir: &Path, args: &[&str]) -> String {
    let output = git_command(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_command(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("failed to run git")
}

/// Initializes a git repository in `dir` on the fixed [`INITIAL_BRANCH`]
/// with one empty commit — the clean base every worktree- and diff-based
/// test starts from.
pub fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", INITIAL_BRANCH]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join(".gitkeep"), "").expect("write .gitkeep");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

/// Writes `contents` to `path`, creating parent directories first — the
/// one-liner every test uses to lay down a workflow, config or fixture
/// file.
pub fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(path, contents).expect("write file");
}
