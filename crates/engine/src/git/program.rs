//! Which `git` a call runs: the first one on the `PATH` the call searches,
//! found once per distinct `PATH` value. On macOS the system `/usr/bin/git`
//! is a shim that resolves the real git on every call, so the git that
//! `xcrun` finds is used instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::{Cause, Stopped};
use crate::process::{spawn_governed, Drain, GovernedCommand, Outcome, Supervision};

/// The program a call runs: the first `git` on its `PATH`, with the
/// system shim swapped for the one `xcrun` finds.
pub(super) async fn program(
    env: &[(String, String)],
    sup: Supervision<'_>,
) -> Result<PathBuf, Cause> {
    let path = path_of(env);
    if let Some(found) = cached(&path) {
        return Ok(found);
    }
    let first = first_on(&path, "git")
        .await
        .ok_or_else(|| not_found(&path))?;
    let shim = cfg!(target_os = "macos") && first == Path::new("/usr/bin/git");
    let found = if shim {
        xcrun_git(sup).await?.unwrap_or(first)
    } else {
        first
    };
    cache(path, found.clone());
    Ok(found)
}

/// The `PATH` a call searches: the one its variables set (the last, when
/// they set it more than once), or else the process's own.
fn path_of(env: &[(String, String)]) -> String {
    match env.iter().rev().find(|(name, _)| name == "PATH") {
        Some((_, value)) => value.clone(),
        None => crate::process::inherited_path(),
    }
}

/// The first file called `name` in the directories `path` lists, in the
/// order a search for a program takes them. A file nobody may execute is
/// passed over, as the system's own search passes it over.
async fn first_on(path: &str, name: &str) -> Option<PathBuf> {
    for dir in std::env::split_paths(path) {
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

/// The programs found so far, keyed by the `PATH` each search ran under.
static FOUND: Mutex<BTreeMap<String, PathBuf>> = Mutex::new(BTreeMap::new());

fn cached(path: &str) -> Option<PathBuf> {
    FOUND
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(path)
        .cloned()
}

fn cache(path: String, program: PathBuf) {
    FOUND
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path, program);
}

/// The refusal for a `PATH` with no `git` on it: the spawn that cannot
/// find its program.
fn not_found(path: &str) -> Cause {
    Cause::Spawn(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("no git on the PATH `{path}`"),
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
        let path = std::env::join_paths([&first, &second]).unwrap();
        let found = first_on(path.to_str().unwrap(), "git").await;
        assert_eq!(found, Some(second.join("git")));
    }
}
