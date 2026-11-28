//! See [`super`]. How a pack reaches a project: its repository cloned from
//! where its source names, `<repository>//<directory>[@ref]` (`source` reads
//! the form and holds the directory inside the clone), its manifest read at
//! that directory, its tree there, as its walk found it, hashed and vendored
//! through a staging directory beside its place, and the `yunta.lock` it is
//! recorded in, with its source and directory, read and written whole. What
//! fails here is a [`PackError`], said in its own words.

use std::path::{Path, PathBuf};

use yunta_core::durable::{self, Durability};
use yunta_core::{sha256_hex, CommitSha, ContentHash, PackLock, PackManifest, PackRef};
use yunta_engine::process::Supervision;
use yunta_engine::{GitError, PackTree, PackWalkError};

use super::source::Checkout;

use crate::{Remark, Remarks};

/// What failed while a pack was fetched, vendored or recorded: git, the
/// disk, or a manifest that does not read.
#[derive(Debug, thiserror::Error)]
pub(super) enum PackError {
    #[error(transparent)]
    Git(GitError),
    #[error("failed to read `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{path}` has no `pack.yaml` at its root: not a pack")]
    NoManifest { path: PathBuf },
    #[error("`{path}`: {detail}")]
    InvalidManifest { path: PathBuf, detail: String },
    #[error("`{path}` isn't valid UTF-8, can't be hashed as pack content")]
    NonUtf8Path { path: PathBuf },
    #[error("`git rev-parse HEAD` answered `{line}`, which is not a commit")]
    NotACommit {
        line: String,
        #[source]
        source: yunta_core::InvalidId,
    },
    #[error("{}", yunta_render::page::pack::link_in_its_tree(path))]
    Symlink { path: PathBuf },
    #[error("pack.yaml: {detail}")]
    Manifest { detail: String },
    #[error("{cause}; the cleanup also failed: {cleanup}")]
    Cleanup {
        #[source]
        cause: Box<PackError>,
        cleanup: Box<PackError>,
    },
}

async fn run_git(
    cwd: &Path,
    args: &[&str],
    supervision: Supervision<'_>,
) -> Result<String, PackError> {
    yunta_engine::git::read(cwd, args, supervision)
        .await
        .map(|stdout| stdout.trim().to_string())
        .map_err(PackError::Git)
}

/// Clones `url` into `dest` (a fresh, empty directory), with `checkout` checked
/// out: a tag known to be one alone, at depth one; else the whole repository,
/// since a ref named can be a commit, which a clone at depth one cannot reach,
/// and the ref checked out in it. Each command ends its options before the
/// url and the ref, so neither is read as one. A local `url` written relative
/// is read from `from`, the repository the pack is installed in.
pub(super) async fn clone_pack(
    url: &str,
    checkout: Checkout<'_>,
    from: &Path,
    dest: &Path,
    supervision: Supervision<'_>,
) -> Result<(), PackError> {
    let dest_arg = dest.display().to_string();
    let clone: &[&str] = match checkout {
        Checkout::Tag(tag) => &["clone", "--quiet", "--depth", "1", "--branch", tag],
        Checkout::Landed | Checkout::Ref(_) => &["clone", "--quiet"],
    };
    let clone = [clone, &["--", url, &dest_arg]].concat();
    run_git(from, &clone, supervision).await?;
    if let Checkout::Ref(reference) = checkout {
        let checked_out = ["checkout", "--quiet", "--end-of-options", reference];
        run_git(dest, &checked_out, supervision).await?;
    }
    Ok(())
}

/// The commit `dest` (an already-cloned working tree) currently has
/// checked out — what `add`/`update` records as the lock entry's
/// `commit`, independent of whether `ref` itself later moves.
pub(super) async fn head_commit(
    dest: &Path,
    supervision: Supervision<'_>,
) -> Result<CommitSha, PackError> {
    let line = run_git(dest, &["rev-parse", "HEAD"], supervision).await?;
    line.parse()
        .map_err(|source| PackError::NotACommit { line, source })
}

/// The branch `dest` landed on when no explicit ref was requested — the
/// descriptive `ref` a lock entry records for a plain `pack add <url>`
/// with no `@ref` suffix, so `yunta.lock` never has to say "unknown".
pub(super) async fn current_branch(
    dest: &Path,
    supervision: Supervision<'_>,
) -> Result<String, PackError> {
    run_git(dest, &["rev-parse", "--abbrev-ref", "HEAD"], supervision).await
}

/// Reads, parses and validates `<dir>/pack.yaml`: a manifest whose
/// names or paths would escape the pack is refused here, before any
/// command acts on it.
pub(super) fn read_manifest(dir: &Path) -> Result<PackManifest, PackError> {
    let manifest = parse_manifest(dir)?;
    let violations = manifest.validate();
    if violations.is_empty() {
        Ok(manifest)
    } else {
        Err(PackError::Manifest {
            detail: violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        })
    }
}

fn parse_manifest(dir: &Path) -> Result<PackManifest, PackError> {
    let path = dir.join("pack.yaml");
    let contents = std::fs::read_to_string(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            PackError::NoManifest {
                path: dir.to_path_buf(),
            }
        } else {
            PackError::Read {
                path: path.clone(),
                source,
            }
        }
    })?;
    PackManifest::read(&contents).map_err(|e| PackError::InvalidManifest {
        path,
        detail: e.to_string(),
    })
}

/// sha256 over every file under `dir` (skipping `.git`), each
/// contributing `"<relative/path>\0<sha256 of its bytes>\n"` in
/// lexicographic path order — deterministic regardless of the
/// filesystem's own directory-entry order, and independent of file
/// mtimes/permissions surviving a copy (only path + content matter,
/// same as any other content-addressed hash in this codebase). A
/// symbolic link anywhere in the tree is an error, never followed.
pub(super) fn hash_tree(dir: &Path) -> Result<ContentHash, PackError> {
    let tree = walked(dir)?;
    let mut digest_input = String::new();
    for relative in tree.files() {
        let full = dir.join(relative);
        let bytes = std::fs::read(&full).map_err(|source| PackError::Read {
            path: full.clone(),
            source,
        })?;
        let relative_str = relative.to_str().ok_or_else(|| PackError::NonUtf8Path {
            path: relative.to_path_buf(),
        })?;
        digest_input.push_str(relative_str);
        digest_input.push('\0');
        digest_input.push_str(sha256_hex(&bytes).as_str());
        digest_input.push('\n');
    }
    Ok(sha256_hex(digest_input.as_bytes()))
}

/// The tree of the pack at `dir`, walked: refused at a symbolic link anywhere
/// in it, which a pack vendored here holds none of.
pub(super) fn walked(dir: &Path) -> Result<PackTree, PackError> {
    PackTree::walk(dir).map_err(|walked| match walked {
        PackWalkError::Link { link } => PackError::Symlink { path: link },
        PackWalkError::Read { path, source } => PackError::Read { path, source },
    })
}

/// Copies every file of `tree` (its walk passed `.git` over) into `dest`,
/// creating directories as needed — the vendoring step itself,
/// separate from the clone so `add`/`update` can hash the clone (which
/// still has `.git`) and vendor a `.git`-free copy from the same
/// checkout without cloning twice.
fn vendor_tree(tree: &PackTree, dest: &Path) -> Result<(), PackError> {
    for relative in tree.files() {
        let from = tree.root().join(relative);
        let to = dest.join(relative);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PackError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::copy(&from, &to).map_err(|source| PackError::Write { path: to, source })?;
    }
    Ok(())
}

pub(super) fn packs_root(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/packs")
}

pub(super) fn vendor_dir(cwd: &Path, pack: &PackRef) -> PathBuf {
    packs_root(cwd)
        .join(pack.publisher().as_str())
        .join(pack.name().as_str())
}

/// Where a pack is vendored before it is moved into place: a sibling of
/// its final directory, so the final move is a rename on one
/// filesystem, and a name no pack can have (`.staging-` is not a path
/// segment a pack name can start with — it would begin with a dot the
/// catalog never lists).
pub(super) fn staging_dir(cwd: &Path, pack: &PackRef) -> PathBuf {
    packs_root(cwd)
        .join(pack.publisher().as_str())
        .join(format!(".staging-{}-{}", pack.name(), std::process::id()))
}

/// A tree moved into place. The tree it replaced stays on disk until the
/// caller knows the whole install, the lock included, has landed.
#[must_use]
pub(super) struct Swap {
    dest: PathBuf,
    previous: Option<PathBuf>,
}

impl Swap {
    /// The install failed after the move: the installed tree is removed, and
    /// the replaced tree, when there was one, goes back to `dest`.
    pub(super) fn undo(self) -> Result<(), PackError> {
        // Nothing is there when the move never reached `dest`.
        Swap::discard(&self.dest)?;
        let Some(previous) = self.previous else {
            return Ok(());
        };
        std::fs::rename(&previous, &self.dest).map_err(|source| PackError::Write {
            path: self.dest,
            source,
        })
    }

    /// The install landed: the replaced tree is dropped. A failure here is
    /// said through `remarks`, because the installed tree is already in
    /// place.
    pub(super) fn commit(self, remarks: &dyn Remarks) {
        let Some(previous) = self.previous else {
            return;
        };
        if let Err(error) = std::fs::remove_dir_all(&previous) {
            remarks.remark(Remark::ReplacedTreeKept {
                at: previous,
                said: error.to_string(),
            });
        }
    }

    /// Removes the tree at `staging` when there is one; a failure is the
    /// caller's to report.
    fn discard(staging: &Path) -> Result<(), PackError> {
        match std::fs::remove_dir_all(staging) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(PackError::Write {
                path: staging.to_path_buf(),
                source,
            }),
        }
    }

    /// Moves the tree at `dest` aside for `remove`: nothing is deleted until
    /// `commit`. `None` when there is no tree to move.
    pub(super) fn aside(dest: &Path) -> Result<Option<Swap>, PackError> {
        let previous = dest.with_file_name(format!(
            ".removed-{}-{}",
            dest.file_name().and_then(|n| n.to_str()).unwrap_or("pack"),
            std::process::id()
        ));
        match std::fs::rename(dest, &previous) {
            Ok(()) => Ok(Some(Swap {
                dest: dest.to_path_buf(),
                previous: Some(previous),
            })),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(PackError::Write {
                path: dest.to_path_buf(),
                source,
            }),
        }
    }
}

/// The error of a step that failed, joined with the error of the cleanup
/// that followed it when that cleanup failed too.
pub(super) fn with_cleanup(cause: PackError, cleanup: Result<(), PackError>) -> PackError {
    match cleanup {
        Ok(()) => cause,
        Err(cleanup) => PackError::Cleanup {
            cause: Box::new(cause),
            cleanup: Box::new(cleanup),
        },
    }
}

/// Vendors the files of `tree` into `dest` through a staging directory beside it: the
/// whole tree is copied first and renamed into place last, so `dest` either
/// does not exist or holds the complete pack. A failure removes the staging
/// tree and puts `dest` back as it was; a cleanup that fails too is joined to
/// the error. A `dest` that already exists is replaced only after the new
/// tree is complete, and the replaced tree stays on disk until the returned
/// `Swap` is committed.
pub(super) fn vendor_into_place(
    tree: &PackTree,
    staging: &Path,
    dest: &Path,
) -> Result<Swap, PackError> {
    stage_and_swap(tree, staging, dest).map_err(|cause| with_cleanup(cause, Swap::discard(staging)))
}

fn stage_and_swap(tree: &PackTree, staging: &Path, dest: &Path) -> Result<Swap, PackError> {
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PackError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    vendor_tree(tree, staging)?;
    let previous = dest.with_file_name(format!(
        ".previous-{}-{}",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("pack"),
        std::process::id()
    ));
    let had_previous = dest.exists();
    if had_previous {
        std::fs::rename(dest, &previous).map_err(|source| PackError::Write {
            path: dest.to_path_buf(),
            source,
        })?;
    }
    let swap = Swap {
        dest: dest.to_path_buf(),
        previous: had_previous.then_some(previous),
    };
    match std::fs::rename(staging, dest) {
        Ok(()) => Ok(swap),
        Err(source) => {
            let cause = PackError::Write {
                path: dest.to_path_buf(),
                source,
            };
            // Without a replaced tree, `dest` was never this swap's to clear.
            if !had_previous {
                return Err(cause);
            }
            Err(with_cleanup(cause, swap.undo()))
        }
    }
}

pub(super) fn lock_path(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/yunta.lock")
}

/// The file a pack command holds the repository's packs through, beside the
/// lock it reads and writes: it exists only while one does.
pub(super) fn hold_path(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/yunta.lock.held")
}

pub(super) fn load_lock(cwd: &Path) -> Result<PackLock, PackError> {
    let path = lock_path(cwd);
    match std::fs::read(&path) {
        Ok(bytes) => {
            PackLock::read(&bytes)
                .map(|lock| lock.doc)
                .map_err(|e| PackError::InvalidManifest {
                    path,
                    detail: yunta_core::describe(&e),
                })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PackLock::default()),
        Err(source) => Err(PackError::Read { path, source }),
    }
}

pub(super) fn save_lock(cwd: &Path, lock: &PackLock) -> Result<(), PackError> {
    let path = lock_path(cwd);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PackError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let yaml = yunta_core::persisted::PersistedDoc::of(lock.clone())
        .write()
        .map_err(|e| PackError::Manifest {
            detail: format!("cannot serialize yunta.lock: {}", yunta_core::describe(&e)),
        })?;
    // Replaced whole and synced before the call returns: a reader never sees
    // a half-written lock, and a failed write leaves the lock as it was.
    durable::replace(&path, &yaml, Durability::Synced)
        .map_err(|source| PackError::Write { path, source })
}

#[cfg(test)]
mod tests;
