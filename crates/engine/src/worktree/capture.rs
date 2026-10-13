//! Capturing the tree a run or a unit works in, as git hashes it: through a
//! private index that never touches the checkout's own, and the tree a
//! checkout's `HEAD` names. The ignored paths a project protects belong to
//! the tree a close judges: a capture stages them, a commit never holds them,
//! and a checkout given back removes them.

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use yunta_core::protected::Pathspecs;
use yunta_core::{InvalidId, TreeId};

use super::{git, Effect, WorktreeError};
use crate::process::Supervision;

/// The tree `cwd` stands at right now, as the starting point a later
/// audit measures against.
///
/// Captured through an index of its own (`GIT_INDEX_FILE` under
/// `scratch`), never the repository's: a sibling unit working in the
/// same checkout at the same moment must not find this call holding
/// `.git/index`, and this call must not see half of what that sibling
/// was mid-way through staging.
///
/// `index` must sit outside `cwd`, and no two units working in one tree
/// at once may name the same one. Staging is `add -A` over the whole
/// checkout, so an index kept inside the tree it measures ends up in the
/// tree it measures; and a shared index is a lock two captures fight
/// over. `run_dir::index_for` answers both: outside the worktree,
/// named by the unit.
///
/// What git ignores under `covered`, the protected paths, is part of the
/// tree all the same, so it is staged too.
pub async fn capture_tree(
    cwd: &Path,
    index: &Path,
    covered: &Pathspecs,
    supervision: Supervision<'_>,
) -> Result<TreeId, WorktreeError> {
    let index = private_index(index).await?;
    // A private index begins as a copy of the checkout's own, so the
    // first capture hashes only what changed since the checkout last
    // staged anything — not every file it holds.
    if !tokio::fs::try_exists(&index).await.unwrap_or(false) {
        seed_index(cwd, &index, supervision).await;
    }
    // The index travels the way every other value this engine hands a
    // child does: through the supervision it is already governed by.
    let mut env: Vec<(String, String)> = supervision.env.to_vec();
    env.push(("GIT_INDEX_FILE".to_string(), index.display().to_string()));
    let private = supervision.with_env(&env);
    // `add -A` stages what is there, untracked included, so the tree is
    // the whole visible state and not only what git already followed.
    git::change(cwd, Effect::Objects, &["add", "-A"], private).await?;
    force_covered(cwd, covered, &beside(&index, ".ignored"), private).await?;
    let printed = git::change(cwd, Effect::Objects, &["write-tree"], private).await?;
    printed
        .trim()
        .parse()
        .map_err(|source: InvalidId| WorktreeError::NotATree {
            args: "write-tree".to_string(),
            cwd: cwd.to_path_buf(),
            source,
        })
}

/// The tree `cwd` stands at as git sees it, for a commit to hold. It is
/// captured through an index of its own beside `index`, into which no
/// capture forces an ignored path, so a commit never holds what the project
/// ignores.
pub(super) async fn capture_for_commit(
    cwd: &Path,
    index: &Path,
    supervision: Supervision<'_>,
) -> Result<TreeId, WorktreeError> {
    let own = beside(index, ".commit");
    capture_tree(cwd, &own, &Pathspecs::default(), supervision).await
}

/// Force-adds the ignored paths `covered` reaches into the index the
/// supervision names, so the `write-tree` after it holds them.
///
/// The paths reach git through `listing`, since a git call here has no
/// stdin. Literal matching names each path as it is, so a name with glob
/// characters stays one path: a pathspec that matches nothing fails `add`,
/// which is why the paths are listed first and never passed as globs.
async fn force_covered(
    cwd: &Path,
    covered: &Pathspecs,
    listing: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let listed = covered_ignored(cwd, covered, supervision).await?;
    if listed.is_empty() {
        return Ok(());
    }
    tokio::fs::write(listing, &listed)
        .await
        .map_err(|source| WorktreeError::Io {
            action: "write the ignored paths to stage".to_string(),
            path: listing.to_path_buf(),
            source,
        })?;
    let from = format!("--pathspec-from-file={}", listing.display());
    let args = [
        "--literal-pathspecs",
        "add",
        "--force",
        from.as_str(),
        "--pathspec-file-nul",
    ];
    git::change(cwd, Effect::Objects, &args, supervision).await?;
    Ok(())
}

/// Removes from `checkout` what git ignores under `covered`, which the
/// plain `clean -ffd` leaves in place: all of it under a rooted pathspec,
/// and each file a loose one reaches.
pub(super) async fn clean_covered(
    checkout: &Path,
    covered: &Pathspecs,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    if !covered.rooted.is_empty() {
        let mut args = vec!["clean", "-q", "-ffdx", "--"];
        args.extend(covered.rooted.iter().map(String::as_str));
        git::change(checkout, Effect::Checkout, &args, supervision).await?;
    }
    let loose = ignored_under(checkout, &["--directory"], &covered.loose, supervision).await?;
    for file in listed_files(&loose) {
        let path = checkout.join(std::ffi::OsStr::from_bytes(file));
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(WorktreeError::Io {
                    action: "remove an ignored protected file".to_string(),
                    path,
                    source,
                })
            }
        }
    }
    Ok(())
}

/// The ignored paths `covered` reaches, NUL-separated as git lists them:
/// every one under a rooted pathspec, and each file a loose one reaches
/// without entering a directory the project ignores whole.
async fn covered_ignored(
    cwd: &Path,
    covered: &Pathspecs,
    supervision: Supervision<'_>,
) -> Result<Vec<u8>, WorktreeError> {
    let mut listed = ignored_under(cwd, &[], &covered.rooted, supervision).await?;
    let loose = ignored_under(cwd, &["--directory"], &covered.loose, supervision).await?;
    for file in listed_files(&loose) {
        listed.extend_from_slice(file);
        listed.push(0);
    }
    Ok(listed)
}

/// What git ignores under `pathspecs`, listed with `extra` and
/// NUL-separated. Nothing runs for no pathspec, which would list every
/// ignored path in the checkout.
async fn ignored_under(
    cwd: &Path,
    extra: &[&str],
    pathspecs: &[String],
    supervision: Supervision<'_>,
) -> Result<Vec<u8>, WorktreeError> {
    if pathspecs.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![
        "ls-files",
        "-z",
        "--others",
        "--ignored",
        "--exclude-standard",
    ];
    args.extend_from_slice(extra);
    args.push("--");
    args.extend(pathspecs.iter().map(String::as_str));
    Ok(git::read_bytes(cwd, &args, supervision).await?)
}

/// The files of a NUL-separated listing. A directory entry, which
/// `--directory` names with a final `/` instead of entering it, is left out.
fn listed_files(listing: &[u8]) -> impl Iterator<Item = &[u8]> {
    listing
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty() && !entry.ends_with(b"/"))
}

/// A file named after `index` with `suffix` added: beside the private index,
/// which only one capture uses.
fn beside(index: &Path, suffix: &str) -> PathBuf {
    let mut named = index.as_os_str().to_os_string();
    named.push(suffix);
    PathBuf::from(named)
}

/// Copies `cwd`'s own index to `index`. A checkout with no index of its
/// own, or one that cannot be copied, leaves the private index to start
/// from nothing, which costs time and nothing else.
async fn seed_index(cwd: &Path, index: &Path, supervision: Supervision<'_>) {
    let own = git::read(
        cwd,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        supervision,
    )
    .await;
    let copied = match own {
        Ok(own) => tokio::fs::copy(own.trim(), index).await.map(|_| ()),
        Err(error) => Err(std::io::Error::other(error)),
    };
    if let Err(error) = copied {
        tracing::debug!(%error, "a private index starts from nothing");
    }
}

/// `index` made absolute, with its directory in place.
///
/// Absolute before anything touches it: this creates the index's
/// directory and a git child opens the file, and the two resolve a
/// relative path against different directories — here the process's,
/// there the child's. Absolute, they name the one file, and no index can
/// land inside the tree it is measuring by accident.
pub(super) async fn private_index(index: &Path) -> Result<PathBuf, WorktreeError> {
    let index = std::path::absolute(index).map_err(|source| WorktreeError::Io {
        action: "resolve the private index path".to_string(),
        path: index.to_path_buf(),
        source,
    })?;
    if let Some(parent) = index.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the private index's directory".to_string(),
                path: parent.to_path_buf(),
                source,
            })?;
    }
    Ok(index)
}

/// The tree `repo`'s `HEAD` points at — the same question
/// [`super::head_commit`] asks, answered as the object a diff takes as an end.
///
/// What a unit that was handed a clean checkout of its own started
/// from: the commit it was branched from is exactly what it began with,
/// so its starting tree needs no capture. A node sharing a tree it did
/// not receive clean uses [`capture_tree`](fn@capture_tree) instead.
pub async fn head_tree(repo: &Path, supervision: Supervision<'_>) -> Result<TreeId, WorktreeError> {
    let output = git::read(repo, &["rev-parse", "HEAD^{tree}"], supervision).await?;
    output
        .trim()
        .parse()
        .map_err(|source: InvalidId| WorktreeError::NotATree {
            args: "rev-parse HEAD^{tree}".to_string(),
            cwd: repo.to_path_buf(),
            source,
        })
}
