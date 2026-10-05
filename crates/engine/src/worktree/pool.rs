//! The checkouts a project's runs work in, reused from one unit and one
//! run to the next.
//!
//! A build is right only at the path that built it: what a build leaves
//! names the directory it ran in, and build tools judge their own output
//! fresh by time, so a build shared between checkouts — or copied into
//! another — runs one checkout's code in another's. A project instead
//! keeps a pool of checkouts at paths of their own, and every checkout a
//! run works in comes from it: a unit's, a probe's, the measurement's. A
//! checkout is handed out again where it is, put back to the commit its
//! next user starts from, and what git ignores — the build — stays there
//! for that user.
//!
//! A checkout is free when nothing in it is uncommitted, no live process
//! holds it, and nothing it committed is still somebody's: it is on no
//! branch, or its branch's work is in the commit its next user starts
//! from. The attempts of one unit take back the checkout one of them
//! worked in first; work that has not landed keeps its checkout its
//! unit's. Holding a checkout lasts as long as
//! the process ([`slot`]); owning one is the branch it is on, which
//! outlives every process.

mod release;
mod runs;
mod slot;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::CommitSha;

use super::{head_tree, prepare_worktree, Unit, UnitHome, UnitId, WorktreeError};
use crate::lock::Contention;
use crate::process::Supervision;

pub use release::{
    forget_run_units, hand_over_run_checkout, release_run_checkout, release_unit_checkouts,
    retire_unit_branch, still_the_runs, trim_pool,
};
use slot::SLOT;

/// A project's checkouts, as one run's invocation reaches them.
pub struct CheckoutPool {
    /// Where every project keeps its pool.
    root: PathBuf,
    /// A checkout of the project: what its pool is told apart by, and what
    /// a new checkout is added from.
    repo: PathBuf,
    /// The directory of the run taking checkouts through this handle; none
    /// for the pool's own upkeep.
    holder: Option<PathBuf>,
    /// The project's pool directory, read once.
    home: tokio::sync::OnceCell<PathBuf>,
}

/// A hold on one checkout of the pool, given back when dropped.
pub struct Lease {
    lock: Option<PathBuf>,
}

impl Lease {
    fn of(lock: PathBuf) -> Self {
        Lease { lock: Some(lock) }
    }

    /// The hold on a checkout the pool does not keep — one a run made
    /// before its project kept checkouts — which nothing else takes.
    fn outside() -> Self {
        Lease { lock: None }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(lock) = self.lock.take() {
            // blocking: a `Drop` has no `await` to give, and removing one
            // lock file is a single syscall. Doing it here is what makes an
            // early `?` unable to leak the hold.
            if let Err(error) = std::fs::remove_file(&lock) {
                tracing::debug!(%error, lock = %lock.display(), "a checkout's hold outlived it");
            }
        }
    }
}

/// What a new checkout of the pool starts as.
enum Fresh<'a> {
    /// On a new branch cut from `base`.
    Cut {
        branch: &'a str,
        base: &'a CommitSha,
    },
    /// On a branch that already holds somebody's work.
    On { branch: &'a str },
    /// On `base`, with no branch.
    Detached { base: &'a CommitSha },
}

/// Where the pool of the project whose git directory is `common_dir`
/// lives under `root`: one directory per repository, named after it so a
/// person can tell which is which.
pub fn pool_home(root: &Path, common_dir: &Path) -> PathBuf {
    let named = match common_dir.file_name().and_then(|name| name.to_str()) {
        Some(".git") => common_dir.parent().and_then(Path::file_name),
        _ => common_dir.file_name(),
    };
    let name = named.and_then(|name| name.to_str()).unwrap_or("repository");
    let hash = yunta_core::sha256_hex(common_dir.to_string_lossy().as_bytes());
    root.join("pool")
        .join(format!("{name}-{}", &hash.as_str()[..8]))
}

impl CheckoutPool {
    /// The pool of the project `repo` is a checkout of, kept under `root`,
    /// for the run whose directory is `holder`.
    pub fn new(root: &Path, repo: &Path, holder: &Path) -> Arc<Self> {
        Self::reached(root, repo, Some(holder.to_path_buf()))
    }

    /// The same pool, for its own upkeep: what it holds to tidy is never
    /// recorded as used.
    pub fn upkeep(root: &Path, repo: &Path) -> Arc<Self> {
        Self::reached(root, repo, None)
    }

    fn reached(root: &Path, repo: &Path, holder: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            root: root.to_path_buf(),
            repo: repo.to_path_buf(),
            holder,
            home: tokio::sync::OnceCell::new(),
        })
    }

    /// The pool's directory, which exists once the pool has a checkout.
    pub async fn home(&self, supervision: Supervision<'_>) -> Result<PathBuf, WorktreeError> {
        self.home
            .get_or_try_init(|| async {
                let common_dir = super::common_git_dir(&self.repo, supervision).await?;
                let common_dir = tokio::fs::canonicalize(&common_dir)
                    .await
                    .unwrap_or(common_dir);
                Ok(settled(&pool_home(&self.root, &common_dir)).await)
            })
            .await
            .cloned()
    }

    /// Opens `who`'s unit in a checkout of the pool, on a branch of its own
    /// cut from `home.base`: the checkout one of its attempts worked in, a
    /// free one put back to that commit, or a new one when none is free.
    pub async fn open(
        self: &Arc<Self>,
        home: UnitHome<'_>,
        who: UnitId,
        attempt: u32,
        supervision: Supervision<'_>,
    ) -> Result<(Unit, Lease), WorktreeError> {
        let branch = super::unit_branch(home.run_id, &who, attempt);
        let owner = super::unit_branches(home.run_id, &who);
        let (checkout, lease) = match self.take(Some(&owner), home.base, supervision).await? {
            Some((checkout, lease)) => {
                reset(&checkout, &branch, home.base, supervision).await?;
                (checkout, lease)
            }
            None => {
                let fresh = Fresh::Cut {
                    branch: &branch,
                    base: home.base,
                };
                self.add(home.repo, fresh, supervision).await?
            }
        };
        let from = head_tree(&checkout, supervision).await?;
        let unit = Unit {
            who,
            worktree: checkout,
            base: home.base.clone(),
            from,
        };
        Ok((unit, lease))
    }

    /// A checkout of the pool on `base` with no branch of its own — what a
    /// probe or the measurement runs in — held for its caller. The free one
    /// nearest to `base` is put back to it; a new one is added when none is
    /// free.
    pub async fn open_detached(
        self: &Arc<Self>,
        base: &CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        match self.take(None, base, supervision).await? {
            Some((checkout, lease)) => {
                let detach = [
                    "switch",
                    "--discard-changes",
                    "-q",
                    "--detach",
                    base.as_str(),
                ];
                mutating(&checkout, &detach, supervision).await?;
                crate::git::output(&checkout, &["clean", "-q", "-ffd"], supervision).await?;
                Ok((checkout, lease))
            }
            None => {
                let repo = self.repo.clone();
                self.add(&repo, Fresh::Detached { base }, supervision).await
            }
        }
    }

    /// Holds `checkout` for a unit that reopened it, so nobody else is
    /// given it while that unit works there. Another process may be
    /// looking at it for a moment, so this waits that moment out.
    pub async fn hold(
        self: &Arc<Self>,
        checkout: PathBuf,
        supervision: Supervision<'_>,
    ) -> Result<Lease, WorktreeError> {
        let home = self.home(supervision).await?;
        let checkout = tokio::fs::canonicalize(&checkout).await.unwrap_or(checkout);
        let number = checkout
            .parent()
            .filter(|parent| *parent == home)
            .and_then(|_| {
                checkout
                    .file_name()?
                    .to_str()?
                    .strip_prefix(SLOT)?
                    .parse()
                    .ok()
            });
        let Some(number) = number else {
            return Ok(Lease::outside());
        };
        let moment = Contention::Wait {
            patience: std::time::Duration::from_secs(5),
            poll: std::time::Duration::from_millis(15),
        };
        slot::hold(&home, number, moment, self.holder.as_deref(), supervision)
            .await?
            .ok_or(WorktreeError::CheckoutHeld { path: checkout })
    }

    /// The most fitting checkout for a user owning the branches starting
    /// with `owner` and starting from `target`, held for it.
    async fn take(
        &self,
        owner: Option<&str>,
        target: &CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<Option<(PathBuf, Lease)>, WorktreeError> {
        let home = self.home(supervision).await?;
        let mut ranked = Vec::new();
        for (number, path) in slot::listed(&home).await {
            if let Some(rank) = slot::rank(&path, owner, target, supervision).await {
                ranked.push((rank, number, path));
            }
        }
        ranked.sort();
        for (_, number, path) in ranked {
            let held = slot::hold(
                &home,
                number,
                Contention::Refuse,
                self.holder.as_deref(),
                supervision,
            );
            let Some(lease) = held.await? else {
                continue;
            };
            // Held, then looked at again: another process may have taken
            // and changed it between the look that ranked it and the hold.
            if slot::rank(&path, owner, target, supervision)
                .await
                .is_some()
            {
                return Ok(Some((path, lease)));
            }
        }
        Ok(None)
    }

    /// Adds a checkout to the pool under the lowest number nobody uses,
    /// held for its caller.
    async fn add(
        &self,
        repo: &Path,
        fresh: Fresh<'_>,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        let home = self.home(supervision).await?;
        tokio::fs::create_dir_all(&home)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the project's checkouts directory".to_string(),
                path: home.clone(),
                source,
            })?;
        let mut number = 0;
        loop {
            number += 1;
            let path = home.join(format!("{SLOT}{number}"));
            if slot::is_checkout(&path, supervision).await {
                continue;
            }
            let held = slot::hold(
                &home,
                number,
                Contention::Refuse,
                self.holder.as_deref(),
                supervision,
            );
            let Some(lease) = held.await? else {
                continue;
            };
            if tokio::fs::try_exists(&path).await.unwrap_or(false) {
                // Added by another process since the look above, or left
                // half added by one that crashed.
                if slot::is_checkout(&path, supervision).await {
                    continue;
                }
                slot::clear(repo, &path, supervision).await?;
            }
            make(repo, &path, fresh, supervision).await?;
            let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);
            return Ok((path, lease));
        }
    }
}

/// `path` named the way the file system names it — every link resolved —
/// whether or not it exists: what exists of it is resolved, and the
/// rest follows as written. A checkout is named by its resolved path, so
/// the pool's own directory has to be too.
async fn settled(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(resolved) = tokio::fs::canonicalize(&existing).await {
            return rest.iter().rev().fold(resolved, |at, part| at.join(part));
        }
        match (
            existing.file_name().map(ToOwned::to_owned),
            existing.parent(),
        ) {
            (Some(part), Some(parent)) => {
                rest.push(part);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Puts `checkout` back to the commit it is on, with nothing a probe
/// wrote left in it but what git ignores — so it is free again.
pub async fn put_back(checkout: &Path, supervision: Supervision<'_>) -> Result<(), WorktreeError> {
    crate::git::output(checkout, &["reset", "-q", "--hard"], supervision).await?;
    crate::git::output(checkout, &["clean", "-q", "-ffd"], supervision).await?;
    Ok(())
}

/// Makes the checkout of `repo` at `path` that `fresh` says.
async fn make(
    repo: &Path,
    path: &Path,
    fresh: Fresh<'_>,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    match fresh {
        Fresh::Cut { branch, base } => {
            let isolation = yunta_core::Isolation::Worktree;
            prepare_worktree(repo, path, base, branch, isolation, supervision).await?;
            Ok(())
        }
        Fresh::On { branch } => add(repo, path, &[branch], supervision).await,
        Fresh::Detached { base } => {
            add(repo, path, &["--detach", base.as_str()], supervision).await
        }
    }
}

/// Adds a checkout of `repo` at `checkout`, on what `on` names: a branch,
/// or `--detach` and a commit.
async fn add(
    repo: &Path,
    checkout: &Path,
    on: &[&str],
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    if let Some(parent) = checkout.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| WorktreeError::Io {
                action: "create the worktrees directory".to_string(),
                path: parent.to_path_buf(),
                source,
            })?;
    }
    let common_dir = super::common_git_dir(repo, supervision).await?;
    let _mutation_lock = super::lock_worktree_mutations(&common_dir, supervision.clock).await?;
    let path = checkout.display().to_string();
    let mut args = vec!["worktree", "add", path.as_str()];
    args.extend_from_slice(on);
    super::run_git(repo, &args, supervision).await?;
    Ok(())
}

/// Runs git `args` in `cwd` with every worktree mutation of its repository
/// held off. A switch that cuts, moves or lets go of a branch, and a
/// branch deletion, read every checkout's metadata — which an `add` beside
/// them may have half written, and git then fails to read — so they take
/// the lock an `add` takes.
pub(super) async fn mutating(
    cwd: &Path,
    args: &[&str],
    supervision: Supervision<'_>,
) -> Result<String, WorktreeError> {
    let common_dir = match common_dir_of(cwd).await {
        Some(found) => found,
        None => super::common_git_dir(cwd, supervision).await?,
    };
    let _mutation_lock = super::lock_worktree_mutations(&common_dir, supervision.clock).await?;
    Ok(crate::git::output(cwd, args, supervision).await?)
}

/// The common git directory of the checkout at `cwd`, read off its `.git`
/// as git reads it — `None` when that does not answer, and git is asked.
async fn common_dir_of(cwd: &Path) -> Option<PathBuf> {
    let dot_git = cwd.join(".git");
    if tokio::fs::metadata(&dot_git).await.ok()?.is_dir() {
        return Some(dot_git);
    }
    let pointer = tokio::fs::read_to_string(&dot_git).await.ok()?;
    let admin = cwd.join(pointer.strip_prefix("gitdir:")?.trim());
    let common = tokio::fs::read_to_string(admin.join("commondir"))
        .await
        .ok()?;
    Some(admin.join(common.trim()))
}

/// Puts `checkout` on a new `branch` at `base`, with nothing of the unit
/// that worked there before but what git ignores.
async fn reset(
    checkout: &Path,
    branch: &str,
    base: &CommitSha,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let cut = [
        "switch",
        "--discard-changes",
        "-q",
        "-C",
        branch,
        base.as_str(),
    ];
    mutating(checkout, &cut, supervision).await?;
    crate::git::output(checkout, &["clean", "-q", "-ffd"], supervision).await?;
    let request = checkout.join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
    match tokio::fs::remove_file(&request).await {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => Err(WorktreeError::Io {
            action: "clear the scope request a unit left".to_string(),
            path: request,
            source,
        }),
        _ => Ok(()),
    }
}
