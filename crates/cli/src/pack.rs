//! Support for `yunta pack add/remove/list/update`: git-clone-and-vendor
//! mechanics, the `yunta.lock` on-disk convention, and the vendored
//! tree's content hash. The command functions
//! (`commands::pack`) do the printing and argument handling; this
//! module is where the actual filesystem/git work lives, kept separate
//! so it stays testable without going through a CLI process.

use std::path::{Path, PathBuf};
use yunta_core::CommitSha;
use yunta_engine::process::Supervision;

use crate::error::warn;
use yunta_core::durable::{self, Durability};
use yunta_core::{sha256_hex, ContentHash, PackLock, PackManifest, PackRef};

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("{}", yunta_core::text::detailed(format!("git {args}"), .detail))]
    Git { args: String, detail: String },
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
    #[error("`{path}` has no `pack.yaml` at its root — not a pack")]
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
    #[error(
        "`{path}` is a symlink — a pack ships regular files only, so vendoring never follows a \
         link out of the pack or copies what one points at"
    )]
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

/// `<source>[@<ref>]` (e.g. `github.com/acme/review-pack@v1.2.0`) —
/// split on the last `@` **only when it follows the source's last `/`**,
/// so an SSH shorthand like `git@github.com:acme/repo.git` (whose `@`
/// comes before any `/`) is never mistaken for a `source@ref` pin.
pub fn split_source_and_ref(spec: &str) -> (&str, Option<&str>) {
    let last_slash = spec.rfind('/');
    let at = spec.rfind('@');
    match (at, last_slash) {
        (Some(at), Some(slash)) if at > slash => (&spec[..at], Some(&spec[at + 1..])),
        (Some(at), None) => (&spec[..at], Some(&spec[at + 1..])),
        _ => (spec, None),
    }
}

/// A bare `host/path` shorthand (no scheme, no SSH `user@host:` prefix,
/// not already a local filesystem path) gets `https://` prepended —
/// `github.com/acme/review-pack` is exactly this shorthand, never a
/// literally clonable URL on its own. A local path
/// (starts with `.`, `/` or `~`) or anything already carrying a scheme
/// or an `@` (SSH shorthand) passes through untouched.
pub fn clone_url(source: &str) -> String {
    let looks_local = source.starts_with('.') || source.starts_with('/') || source.starts_with('~');
    let has_scheme = source.contains("://");
    let looks_ssh_shorthand = source.contains('@');
    if looks_local || has_scheme || looks_ssh_shorthand {
        source.to_string()
    } else {
        format!("https://{source}")
    }
}

async fn run_git(
    cwd: &Path,
    args: &[&str],
    supervision: Supervision<'_>,
) -> Result<String, PackError> {
    yunta_engine::git::output(cwd, args, supervision)
        .await
        .map(|stdout| stdout.trim().to_string())
        .map_err(|e| {
            let detail = e.detail();
            PackError::Git {
                args: e.args,
                detail,
            }
        })
}

/// Clones `url` into `dest` (a fresh, empty directory), checking out
/// `ref_` when given — a full clone, not shallow: `--depth 1` would
/// only work for a ref that's a branch tip, and a ref here can just as
/// well be a tag or a commit-ish.
pub async fn clone_pack(
    url: &str,
    ref_: Option<&str>,
    dest: &Path,
    supervision: Supervision<'_>,
) -> Result<(), PackError> {
    run_git(
        Path::new("."),
        &["clone", "--quiet", url, &dest.display().to_string()],
        supervision,
    )
    .await?;
    if let Some(ref_) = ref_ {
        run_git(dest, &["checkout", "--quiet", ref_], supervision).await?;
    }
    Ok(())
}

/// The commit `dest` (an already-cloned working tree) currently has
/// checked out — what `add`/`update` records as the lock entry's
/// `commit`, independent of whether `ref` itself later moves.
pub async fn head_commit(
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
pub async fn current_branch(
    dest: &Path,
    supervision: Supervision<'_>,
) -> Result<String, PackError> {
    run_git(dest, &["rev-parse", "--abbrev-ref", "HEAD"], supervision).await
}

/// Reads, parses and validates `<dir>/pack.yaml`: a manifest whose
/// names or paths would escape the pack is refused here, before any
/// command acts on it.
pub fn read_manifest(dir: &Path) -> Result<PackManifest, PackError> {
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
    yunta_core::yaml::parse(&contents).map_err(|e| PackError::InvalidManifest {
        path,
        detail: e.to_string(),
    })
}

/// sha256 over every file under `dir` (skipping `.git`), each
/// contributing `"<relative/path>\0<sha256 of its bytes>\n"` in
/// lexicographic path order — deterministic regardless of the
/// filesystem's own directory-entry order, and independent of file
/// mtimes/permissions surviving a copy (only path + content matter,
/// same as any other content-addressed hash in this codebase).
pub fn hash_tree(dir: &Path) -> Result<ContentHash, PackError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort();

    let mut digest_input = String::new();
    for relative in &files {
        let full = dir.join(relative);
        let bytes = std::fs::read(&full).map_err(|source| PackError::Read {
            path: full.clone(),
            source,
        })?;
        let relative_str = relative.to_str().ok_or_else(|| PackError::NonUtf8Path {
            path: relative.clone(),
        })?;
        digest_input.push_str(relative_str);
        digest_input.push('\0');
        digest_input.push_str(sha256_hex(&bytes).as_str());
        digest_input.push('\n');
    }
    Ok(sha256_hex(digest_input.as_bytes()))
}

/// Every regular file under `dir`, relative to `root`, `.git` skipped.
/// A symlink anywhere in the tree is an error, never followed: the
/// entry's own metadata decides, so a link to a directory is refused
/// the same as a link to a file.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), PackError> {
    let entries = std::fs::read_dir(dir).map_err(|source| PackError::Read {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| PackError::Read {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| PackError::Read {
            path: path.clone(),
            source,
        })?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| PackError::Read {
                path: path.clone(),
                source: std::io::Error::other("entry outside the tree being collected"),
            })?
            .to_path_buf();
        if metadata.file_type().is_symlink() {
            return Err(PackError::Symlink { path: relative });
        }
        if metadata.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            out.push(relative);
        }
    }
    Ok(())
}

/// Copies every file under `src` (skipping `.git`) into `dest`,
/// creating directories as needed — the vendoring step itself,
/// separate from the clone so `add`/`update` can hash the clone (which
/// still has `.git`) and vendor a `.git`-free copy from the same
/// checkout without cloning twice.
pub fn vendor_tree(src: &Path, dest: &Path) -> Result<(), PackError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(src, src, &mut files)?;
    for relative in files {
        let from = src.join(&relative);
        let to = dest.join(&relative);
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

pub fn packs_root(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/packs")
}

pub fn vendor_dir(cwd: &Path, pack: &PackRef) -> PathBuf {
    packs_root(cwd)
        .join(pack.publisher().as_str())
        .join(pack.name().as_str())
}

/// Where a pack is vendored before it is moved into place: a sibling of
/// its final directory, so the final move is a rename on one
/// filesystem, and a name no pack can have (`.staging-` is not a path
/// segment a pack name can start with — it would begin with a dot the
/// catalog never lists).
pub fn staging_dir(cwd: &Path, pack: &PackRef) -> PathBuf {
    packs_root(cwd)
        .join(pack.publisher().as_str())
        .join(format!(".staging-{}-{}", pack.name(), std::process::id()))
}

/// A tree moved into place. The tree it replaced stays on disk until the
/// caller knows the whole install, the lock included, has landed.
#[must_use]
pub struct Swap {
    dest: PathBuf,
    previous: Option<PathBuf>,
}

impl Swap {
    /// The install failed after the move: the installed tree is removed, and
    /// the replaced tree, when there was one, goes back to `dest`.
    pub fn undo(self) -> Result<(), PackError> {
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

    /// The install landed: the replaced tree is dropped. A failure here is a
    /// warning, because the installed tree is already in place.
    pub fn commit(self) {
        if let Some(previous) = self.previous {
            if let Err(error) = std::fs::remove_dir_all(&previous) {
                warn(format!(
                    "could not remove the replaced tree at {}: {error}",
                    previous.display()
                ));
            }
        }
    }

    /// Removes the tree at `staging` when there is one; a failure is the
    /// caller's to report.
    pub fn discard(staging: &Path) -> Result<(), PackError> {
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
    pub fn aside(dest: &Path) -> Result<Option<Swap>, PackError> {
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
pub fn with_cleanup(cause: PackError, cleanup: Result<(), PackError>) -> PackError {
    match cleanup {
        Ok(()) => cause,
        Err(cleanup) => PackError::Cleanup {
            cause: Box::new(cause),
            cleanup: Box::new(cleanup),
        },
    }
}

/// Vendors `src` into `dest` through a staging directory beside it: the
/// whole tree is copied first and renamed into place last, so `dest` either
/// does not exist or holds the complete pack. A failure removes the staging
/// tree and puts `dest` back as it was; a cleanup that fails too is joined to
/// the error. A `dest` that already exists is replaced only after the new
/// tree is complete, and the replaced tree stays on disk until the returned
/// `Swap` is committed.
pub fn vendor_into_place(src: &Path, staging: &Path, dest: &Path) -> Result<Swap, PackError> {
    stage_and_swap(src, staging, dest).map_err(|cause| with_cleanup(cause, Swap::discard(staging)))
}

fn stage_and_swap(src: &Path, staging: &Path, dest: &Path) -> Result<Swap, PackError> {
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PackError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    vendor_tree(src, staging)?;
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

pub fn lock_path(cwd: &Path) -> PathBuf {
    cwd.join(".yunta/yunta.lock")
}

pub fn load_lock(cwd: &Path) -> Result<PackLock, PackError> {
    let path = lock_path(cwd);
    match std::fs::read(&path) {
        Ok(bytes) => yunta_core::persisted::PersistedDoc::<PackLock>::read(&bytes)
            .map(|lock| lock.doc)
            .map_err(|e| PackError::InvalidManifest {
                path,
                detail: yunta_core::describe(&e),
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PackLock::default()),
        Err(source) => Err(PackError::Read { path, source }),
    }
}

pub fn save_lock(cwd: &Path, lock: &PackLock) -> Result<(), PackError> {
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
