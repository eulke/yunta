//! Programs a test puts on a run's `PATH` in place of the real ones.
//!
//! A stub exists to reach a state the real program will not hold still
//! for: a `git` that is still running when the test asserts about it, a
//! session that dies the way a crash kills one. It is the real program
//! everywhere else, so what a test asserts is about the engine's
//! governance and not about what git happens to do.

use std::path::{Path, PathBuf};

/// Where the stub publishes its pid, and the subcommand it blocks on.
/// Read by the script, set by the test on the run's `subprocess_vars`.
pub const GIT_BLOCK_ON: &str = "YUNTA_STUB_GIT_BLOCK_ON";
pub const GIT_PID: &str = "YUNTA_STUB_GIT_PID";
/// A file whose existence arms the hold — for a test that wants the
/// second time a run asks git the same question and not the first.
pub const GIT_BLOCK_WHEN: &str = "YUNTA_STUB_GIT_BLOCK_WHEN";
const GIT_REAL: &str = "YUNTA_STUB_GIT_REAL";

/// Writes the `git` stub into `dir/bin` and answers with the variables
/// a run must carry for it to be the `git` its subprocesses find:
/// `PATH` with that directory first, and where the real git lives.
///
/// The test adds [`GIT_BLOCK_ON`] and [`GIT_PID`] itself — which
/// subcommand to hold, and where the pid goes — because that is what
/// the test is about.
pub fn git(dir: &Path) -> Vec<(String, String)> {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create the stub directory");
    let stub = bin.join("git");
    std::fs::write(&stub, include_str!("../stubs/git_stub.sh")).expect("write the git stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
            .expect("make the git stub executable");
    }
    vec![
        (
            "PATH".to_string(),
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        ),
        (GIT_REAL.to_string(), real_git().display().to_string()),
    ]
}

/// Everything a test needs for a `git` that runs for real until it is
/// asked to hold: the stub on `PATH`, which invocation to hold (matched
/// against the subcommand or the first two arguments), where it
/// publishes its pid, and — when the hold must not catch the first time
/// the run asks — the file whose existence arms it.
pub fn git_holding(
    dir: &Path,
    invocation: &str,
    pid: &Path,
    armed_by: Option<&Path>,
) -> Vec<(String, String)> {
    let mut vars = git(dir);
    vars.push((GIT_BLOCK_ON.to_string(), invocation.to_string()));
    vars.push((GIT_PID.to_string(), pid.display().to_string()));
    if let Some(armed_by) = armed_by {
        vars.push((GIT_BLOCK_WHEN.to_string(), armed_by.display().to_string()));
    }
    vars
}

/// The git the stub delegates to: the first one on this process's own
/// `PATH`, resolved now so the stub never finds itself.
fn real_git() -> PathBuf {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("a git on this machine's PATH")
}
