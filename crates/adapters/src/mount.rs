//! Mounting a session's skills where its CLI discovers them.
//!
//! A CLI finds skills in a directory of its own under the session's
//! working directory; mounting one is a symlink there named after the
//! skill's directory. The launch declares every link it makes as staged,
//! so the engine's scope check never reads one as the agent's work. The
//! launch holds the mounts for as long as the session runs, and releasing
//! them, or dropping them, takes back exactly what the install made.

use std::path::{Path, PathBuf};

use yunta_core::port::SessionRequest;
use yunta_core::process::subprocess::Held;
use yunta_core::{AdapterError, AdapterId, Result};

/// One skill link a launch stages: `mount` under the session's working
/// directory, pointing at the skill's directory.
pub(crate) struct SkillLink {
    pub(crate) mount: PathBuf,
    pub(crate) skill: PathBuf,
}

/// The links a launch mounts, as data. Building it writes nothing;
/// `install` writes it.
pub(crate) struct MountPlan {
    cwd: PathBuf,
    links: Vec<SkillLink>,
}

impl MountPlan {
    /// Pure: each skill directory of `req`, named after the directory, under `root`.
    pub(crate) fn of(root: &str, req: &SessionRequest) -> Self {
        let links = req
            .skills
            .iter()
            .filter_map(|skill| {
                let name = skill.file_name()?;
                Some(SkillLink {
                    mount: Path::new(root).join(name),
                    skill: skill.clone(),
                })
            })
            .collect();
        MountPlan {
            cwd: req.cwd.clone(),
            links,
        }
    }

    /// The worktree-relative paths of the links, which the scope check leaves out.
    pub(crate) fn staged(&self) -> Vec<PathBuf> {
        self.links.iter().map(|link| link.mount.clone()).collect()
    }

    /// Every link or none. A path that holds anything but a link is refused and left alone.
    pub(crate) fn install(self, adapter: &'static AdapterId) -> Result<Mounts> {
        let mut mounts = Mounts {
            adapter,
            made: Vec::new(),
            armed: true,
        };
        for link in &self.links {
            let dest = self.cwd.join(&link.mount);
            mounts.make_dirs(&dest)?;
            match std::fs::symlink_metadata(&dest) {
                // A stale link is replaced by the new one.
                Ok(meta) if meta.file_type().is_symlink() => {
                    std::fs::remove_file(&dest).map_err(|source| {
                        io_error(adapter, format!("replace {}", dest.display()), source)
                    })?
                }
                Ok(_) => {
                    let source =
                        std::io::Error::new(std::io::ErrorKind::AlreadyExists, "not a link");
                    let action = format!("mount the skill at {}", dest.display());
                    return Err(io_error(adapter, action, source));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    let action = format!("inspect {}", dest.display());
                    return Err(io_error(adapter, action, source));
                }
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link.skill, &dest).map_err(|source| {
                io_error(
                    adapter,
                    format!("stage skill at {}", dest.display()),
                    source,
                )
            })?;
            mounts.made.push(dest);
        }
        Ok(mounts)
    }
}

/// What one install made: its links and the directories it created. Releasing
/// it, or dropping it, takes them back.
pub(crate) struct Mounts {
    adapter: &'static AdapterId,
    made: Vec<PathBuf>,
    armed: bool,
}

impl Mounts {
    /// Creates the directories above `dest` that are missing, recording each one.
    fn make_dirs(&mut self, dest: &Path) -> Result<()> {
        let missing: Vec<&Path> = dest
            .ancestors()
            .skip(1)
            .take_while(|dir| std::fs::symlink_metadata(dir).is_err())
            .collect();
        for dir in missing.into_iter().rev() {
            std::fs::create_dir(dir).map_err(|source| {
                io_error(self.adapter, format!("create {}", dir.display()), source)
            })?;
            self.made.push(dir.to_path_buf());
        }
        Ok(())
    }

    /// Takes back what this made, newest first, and returns the first error.
    /// Once called, nothing is taken back again, so a later drop cannot
    /// remove a link someone else made in the meantime.
    pub(crate) fn release(&mut self) -> Result<()> {
        self.armed = false;
        let mut first: Option<AdapterError> = None;
        for path in std::mem::take(&mut self.made).into_iter().rev() {
            if let Err(source) = take_back(&path) {
                if first.is_none() {
                    let action = format!("take back {}", path.display());
                    first = Some(io_error(self.adapter, action, source));
                }
            }
        }
        first.map_or(Ok(()), Err)
    }
}

impl Held for Mounts {
    fn release(self: Box<Self>) -> Result<()> {
        let mut mounts = *self;
        mounts.release()
    }
}

impl Drop for Mounts {
    fn drop(&mut self) {
        if self.armed {
            // Best effort: a drop cannot return the error, so the debug log keeps it.
            if let Err(error) = self.release() {
                tracing::debug!(%error, "a skill mount was not taken back");
            }
        }
    }
}

/// A link goes. A directory goes only while empty: one someone else put files in is theirs.
fn take_back(path: &Path) -> std::io::Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if meta.file_type().is_symlink() {
        std::fs::remove_file(path)
    } else if meta.is_dir() && std::fs::read_dir(path)?.next().is_none() {
        std::fs::remove_dir(path)
    } else {
        Ok(())
    }
}

fn io_error(adapter: &AdapterId, action: String, source: std::io::Error) -> AdapterError {
    AdapterError::AdapterIo {
        adapter: adapter.clone(),
        action,
        source,
    }
}
