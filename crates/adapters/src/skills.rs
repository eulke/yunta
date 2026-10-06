//! Mounting a session's skills where its CLI discovers them.
//!
//! A CLI finds skills in a directory of its own under the session's
//! working directory; mounting one is a symlink there named after the
//! skill's directory. The adapter declares every link it makes as staged,
//! so the engine's scope check never reads one as the agent's work, and
//! takes them back once the session ends.

use std::path::{Path, PathBuf};

use yunta_core::port::SessionRequest;
use yunta_core::{AdapterError, AdapterId, Result};

/// Where each resolved skill directory is mounted: a link under `root`
/// named after the directory itself. Computed once, for [`stage`] to
/// create and `staged_paths` to declare, so what scope leaves out is
/// exactly what was written.
pub(crate) fn mounts<'a>(root: &str, req: &'a SessionRequest) -> Vec<(PathBuf, &'a Path)> {
    req.skills
        .iter()
        .filter_map(|skill| {
            let name = skill.file_name()?;
            Some((Path::new(root).join(name), skill.as_path()))
        })
        .collect()
}

/// Takes back every link [`stage`] made — only links, so nothing the agent
/// wrote there is touched — and the discovery directories they left
/// empty.
pub(crate) fn unstage(adapter: &AdapterId, root: &str, req: &SessionRequest) -> Result<()> {
    for (mount, _) in mounts(root, req) {
        let dest = req.cwd.join(mount);
        let is_link = std::fs::symlink_metadata(&dest).is_ok_and(|m| m.file_type().is_symlink());
        if is_link {
            std::fs::remove_file(&dest).map_err(|source| AdapterError::AdapterIo {
                adapter: adapter.clone(),
                action: format!("take back the skill at {}", dest.display()),
                source,
            })?;
        }
    }
    // Removed only when empty: a directory someone else put files in is
    // theirs, and so is everything above it.
    let skills_root = req.cwd.join(root);
    for dir in skills_root.ancestors().take(2) {
        if std::fs::remove_dir(dir).is_err() {
            break;
        }
    }
    Ok(())
}

/// Stages a symlink per resolved skill directory under `root`.
/// Re-staging (a retry, a resume) replaces the link.
pub(crate) fn stage(adapter: &AdapterId, root: &str, req: &SessionRequest) -> Result<()> {
    let mounts = mounts(root, req);
    if mounts.is_empty() {
        return Ok(());
    }
    let io_err = |action: String, source: std::io::Error| AdapterError::AdapterIo {
        adapter: adapter.clone(),
        action,
        source,
    };
    let skills_root = req.cwd.join(root);
    std::fs::create_dir_all(&skills_root)
        .map_err(|e| io_err(format!("create {}", skills_root.display()), e))?;
    for (mount, skill) in mounts {
        let dest = req.cwd.join(mount);
        match std::fs::remove_file(&dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(format!("replace {}", dest.display()), e)),
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(skill, &dest)
            .map_err(|e| io_err(format!("stage skill at {}", dest.display()), e))?;
    }
    Ok(())
}
