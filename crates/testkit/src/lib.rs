//! Shared test harness for the Yunta workspace.
//!
//! Every integration test used to carry its own copy of the same
//! scaffolding — a git-repo fixture, a fixed clock, a binary runner, a run
//! bench. This crate holds one canonical copy of each so a change to the
//! shape of a test run happens in one place, and so the fixtures are
//! hermetic by construction (git isolated from the developer's global
//! config, a fixed clock, an injected home) rather than by each test
//! remembering to be.
//!
//! It is a dev-dependency only: nothing here ships in a published crate.

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Utc};
use yunta_core::Clock;

/// Runs `git` in `dir`, asserting success. Global and system git config
/// are pinned to `/dev/null` so a run is identical on every machine — a
/// developer's `~/.gitconfig` (a different `init.defaultBranch`, a hook, a
/// commit template) can never change what a test sees.
pub fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("failed to run git");
    assert!(status.success(), "git {args:?} failed");
}

/// Initializes a git repository in `dir` on a fixed `main` branch with one
/// empty commit — the clean base every worktree- and diff-based test
/// starts from. The branch is named explicitly so the fixture never
/// depends on the host's `init.defaultBranch`.
pub fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
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

/// A [`Clock`] frozen at a fixed instant, so any timestamp a test observes
/// is deterministic and no assertion ever races the wall clock.
pub struct FixedClock;

/// The instant [`FixedClock`] always reports.
pub const FIXED_NOW: &str = "2026-01-01T00:00:00Z";

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(FIXED_NOW)
            .expect("FIXED_NOW is a valid RFC3339 timestamp")
            .with_timezone(&Utc)
    }
}

/// Runs the compiled `yunta` binary at `bin` in `dir` with `YUNTA_HOME`
/// pointed at `home` and stdin closed, returning its captured output. Use
/// the [`yunta_in!`] macro rather than calling this directly — it fills in
/// the binary path from the calling crate's `CARGO_BIN_EXE_yunta`.
pub fn run_yunta(bin: &Path, dir: &Path, home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin)
        .args(args)
        .current_dir(dir)
        .env("YUNTA_HOME", home)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to run the yunta binary")
}

/// Runs the `yunta` binary from an integration test: `yunta_in!(dir, home,
/// &["run", "wf.yaml"])`. The binary path comes from
/// `CARGO_BIN_EXE_yunta`, which Cargo sets only for the tests of the crate
/// that builds the binary — so `env!` is expanded here, at the call site,
/// where that variable exists.
#[macro_export]
macro_rules! yunta_in {
    ($dir:expr, $home:expr, $args:expr) => {
        $crate::run_yunta(
            ::std::path::Path::new(env!("CARGO_BIN_EXE_yunta")),
            $dir,
            $home,
            $args,
        )
    };
}
