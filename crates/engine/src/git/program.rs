//! Which `git` a call runs: the first one on the `PATH` the call searches,
//! found once per distinct `PATH` value. On macOS the system `/usr/bin/git`
//! is a shim that resolves the real git on every call, so the git that
//! `xcrun` finds is used instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::{Cause, Stopped};
use crate::process::{spawn_governed, Drain, GovernedCommand, Outcome, Supervision};

/// The program a call runs: the first `git` on its search path, with the
/// system shim swapped for the one `xcrun` finds. A call that names no
/// search path at all is left to the child's own search, which finds `git`
/// on the `PATH` the child inherits, without the shim swap.
pub(super) async fn program(
    env: &[(String, String)],
    sup: Supervision<'_>,
) -> Result<PathBuf, Cause> {
    let Some(dirs) = search_dirs(env, sup) else {
        return Ok(PathBuf::from("git"));
    };
    if let Some(found) = cached(&dirs) {
        return Ok(found);
    }
    let first = first_on(&dirs, "git")
        .await
        .ok_or_else(|| not_found(&dirs))?;
    let shim = cfg!(target_os = "macos") && first == Path::new("/usr/bin/git");
    let found = if shim {
        xcrun_git(sup).await?.unwrap_or(first)
    } else {
        first
    };
    cache(dirs, found.clone());
    Ok(found)
}

/// The directories a call searches for a program: the `PATH` its variables
/// set (the last, when they set it more than once), or else the one its
/// supervision carries. `None` when neither names one.
fn search_dirs(env: &[(String, String)], sup: Supervision<'_>) -> Option<Vec<PathBuf>> {
    match env.iter().rev().find(|(name, _)| name == "PATH") {
        Some((_, value)) => Some(std::env::split_paths(value).collect()),
        None if sup.path.is_empty() => None,
        None => Some(sup.path.to_vec()),
    }
}

/// The first file called `name` in `dirs`, in the order a search for a
/// program takes them. A file nobody may execute is passed over, as the
/// system's own search passes it over.
async fn first_on(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    for dir in dirs {
        let candidate = dir.join(name);
        let Ok(meta) = tokio::fs::metadata(&candidate).await else {
            continue;
        };
        if meta.is_file() && executable(&meta) {
            return Some(candidate);
        }
    }
    None
}

/// Whether a search for a program would run a file of this mode: one with
/// an execute bit set.
#[cfg(unix)]
fn executable(meta: &impl std::os::unix::fs::MetadataExt) -> bool {
    meta.mode() & 0o111 != 0
}

/// Without execute bits, every file a search finds is one it would run.
#[cfg(not(unix))]
fn executable<T>(_: &T) -> bool {
    true
}

/// The programs found so far, keyed by the directories each search ran in.
static FOUND: Mutex<BTreeMap<Vec<PathBuf>, PathBuf>> = Mutex::new(BTreeMap::new());

fn cached(dirs: &[PathBuf]) -> Option<PathBuf> {
    FOUND
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(dirs)
        .cloned()
}

fn cache(dirs: Vec<PathBuf>, program: PathBuf) {
    FOUND
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(dirs, program);
}

/// The refusal for a search with no `git` in its directories: the spawn
/// that cannot find its program.
fn not_found(dirs: &[PathBuf]) -> Cause {
    let shown = dirs
        .iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    Cause::Spawn(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("no git on the PATH `{shown}`"),
    ))
}

/// The git `xcrun` finds in the active developer directory. `None` when
/// xcrun finds none, so the caller keeps the `git` it found on the `PATH`.
/// A stopped xcrun is a stop, not an absence of git.
async fn xcrun_git(sup: Supervision<'_>) -> Result<Option<PathBuf>, Cause> {
    // xcrun's answer does not depend on the directory it runs in.
    let command = GovernedCommand::new("xcrun", Path::new("/"))
        .arg("--find")
        .arg("git");
    match spawn_governed(command, sup).await {
        Ok(Outcome::Exited {
            status,
            stdout,
            drain: Drain::Complete,
            ..
        }) if status.success() => {
            let found = String::from_utf8_lossy(&stdout).trim().to_string();
            Ok((!found.is_empty()).then(|| PathBuf::from(found)))
        }
        Ok(Outcome::Cancelled { .. }) => Err(Cause::Stopped(Stopped::Cancelled)),
        Ok(Outcome::TimedOut { .. }) => Err(Cause::Stopped(Stopped::TimedOut)),
        Ok(Outcome::Exited { .. }) | Err(_) => Ok(None),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn program_never_resolves_to_the_clt_shim() {
        let cancel = CancellationToken::new();
        let sup = Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);
        let env = [("PATH".to_string(), "/usr/bin:/bin".to_string())];
        // Without the command line tools there is no other git to prefer.
        let Some(clt) = xcrun_git(sup).await.ok().flatten() else {
            return;
        };
        let found = program(&env, sup).await.unwrap();
        assert_ne!(
            found,
            Path::new("/usr/bin/git"),
            "resolved to the system shim"
        );
        assert_eq!(found, clt);
    }
}

#[cfg(test)]
mod search {
    use super::*;
    #[cfg(unix)]
    use std::fs::Permissions;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    /// A `git` nobody may execute is passed over for the next one on the
    /// `PATH`, as the system's own search passes it over.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_git_nobody_may_execute_is_passed_over() {
        let dirs = tempfile::tempdir().unwrap();
        let (first, second) = (dirs.path().join("a"), dirs.path().join("b"));
        for (dir, mode) in [(&first, 0o644), (&second, 0o755)] {
            tokio::fs::create_dir_all(dir).await.unwrap();
            tokio::fs::write(dir.join("git"), "#!/bin/sh\n")
                .await
                .unwrap();
            let permissions = Permissions::from_mode(mode);
            tokio::fs::set_permissions(dir.join("git"), permissions)
                .await
                .unwrap();
        }
        let found = first_on(&[first, second.clone()], "git").await;
        assert_eq!(found, Some(second.join("git")));
    }

    /// A call that names no search path, in its variables or its supervision,
    /// hands the program to the child's own search: the bare name, resolved on
    /// the environment the child inherits.
    #[tokio::test]
    async fn a_call_that_names_no_search_path_leaves_git_to_the_child() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let sup = Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock);
        assert_eq!(program(&[], sup).await.unwrap(), PathBuf::from("git"));
    }

    /// A call that names no search path of its own finds `git` on the search
    /// path its supervision carries, the one the shell read.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_call_finds_git_on_the_search_path_its_supervision_carries() {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join("git");
        tokio::fs::write(&git, "#!/bin/sh\n").await.unwrap();
        tokio::fs::set_permissions(&git, Permissions::from_mode(0o755))
            .await
            .unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let path = vec![dir.path().to_path_buf()];
        let sup =
            Supervision::outside_any_run(&cancel, &yunta_testkit_core::FixedClock).with_path(&path);
        assert_eq!(program(&[], sup).await.unwrap(), git);
    }
}
