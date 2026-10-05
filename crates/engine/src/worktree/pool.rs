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
//! branch and kept for no unit, or the unit it is kept for has its work
//! in the commit its next user starts from. The attempts of one unit take
//! back the checkout kept for it first; work that has not landed keeps
//! its checkout its unit's. Holding a checkout lasts as long as the
//! process ([`slot`]). Owning one outlives every process: a run owns its
//! checkout by its branch, and a unit — which works on no branch, its
//! work kept by its ref — by the checkout's record ([`units`]).
//!
//! A checkout is taken from one survey of the pool ([`survey`], [`take`]):
//! one listing of the repository's checkouts, whatever their number, and
//! a look at the one taken. A checkout given back records its use and the
//! build it leaves ([`give`], [`space`]), which is what ranks it next.

mod add;
mod fit;
mod follow;
mod give;
mod release;
mod room;
mod runs;
mod slot;
mod space;
mod survey;
mod take;
mod tidy;
mod units;
mod walk;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use yunta_core::CommitSha;

use super::{head_tree, Unit, UnitHome, UnitId, WorktreeError};
use crate::process::Supervision;

pub use give::clear_work;
pub use release::{
    adopt_unit_branches, forget_run_units, hand_over_run_checkout, release_run_checkout,
    release_unit_checkouts, still_the_runs,
};
pub use runs::Birth;
use slot::SLOT;
pub use tidy::{PoolReport, Tidied};
pub use units::forget_attempts;

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
    /// The project's common git directory as its run recorded it, which
    /// spares asking git while it is still there.
    known: Option<PathBuf>,
    /// What the pool may keep of the disk.
    limits: yunta_core::PoolConfig,
    /// The walks counting builds this handle started, under way.
    walking: space::Walking,
    /// Where the pool is, read once.
    located: tokio::sync::OnceCell<Located>,
}

/// Where a pool is: its own directory, and the common git directory of
/// the project it keeps checkouts of — what every checkout's change is
/// held off by.
#[derive(Clone)]
struct Located {
    home: PathBuf,
    common_dir: PathBuf,
}

/// What taking a checkout came to.
pub(super) enum Taken {
    /// A checkout that fits its user, held for it.
    Held(PathBuf, Lease),
    /// None fits: the numbers of the checkouts the survey found, which a
    /// new checkout does not take.
    Nothing {
        listed: std::collections::HashSet<u32>,
    },
}

/// A hold on one checkout of the pool, let go when dropped. A user done
/// with the checkout gives it back instead
/// ([`CheckoutPool::give_back`]), which records the use and the build it
/// leaves.
pub struct Lease {
    /// The pool the checkout is one of.
    home: PathBuf,
    /// Its number there.
    number: u32,
}

impl Lease {
    /// Records that the checkout held is kept for `unit` — or for nobody.
    pub(super) async fn keep_for(&self, unit: Option<&super::UnitRef>) {
        slot::owned(&self.home, self.number, unit).await;
    }

    /// The checkout held.
    pub(super) fn checkout(&self) -> PathBuf {
        self.home.join(format!("{SLOT}{}", self.number))
    }

    fn of(home: &Path, number: u32) -> Self {
        Lease {
            home: home.to_path_buf(),
            number,
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let lock = self.home.join(format!("{SLOT}{}.lock", self.number));
        // blocking: a `Drop` has no `await` to give, and removing one
        // lock file is a single syscall. Doing it here is what makes an
        // early `?` unable to leak the hold.
        if let Err(error) = std::fs::remove_file(&lock) {
            tracing::debug!(%error, lock = %lock.display(), "a checkout's hold outlived it");
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
        Self::reached(root, repo, Some(holder.to_path_buf()), None, None)
    }

    /// The pool of the project a run's manifest names, for that run: what
    /// the manifest recorded of the project is taken rather than asked
    /// for again, and what it may keep of the disk is the run's config's.
    pub fn of_run(
        root: &Path,
        repo: &Path,
        holder: &Path,
        manifest: &yunta_core::Manifest,
    ) -> Arc<Self> {
        let known = manifest
            .project
            .as_ref()
            .map(|project| project.git_common_dir.clone());
        let limits = manifest
            .config
            .storage
            .as_ref()
            .and_then(|storage| storage.pool.as_ref());
        Self::reached(root, repo, Some(holder.to_path_buf()), known, limits)
    }

    /// The pool of the project `repo` is a checkout of, for the run whose
    /// directory is `holder`, kept within `limits`.
    pub fn within(
        root: &Path,
        repo: &Path,
        holder: &Path,
        limits: &yunta_core::PoolConfig,
    ) -> Arc<Self> {
        Self::reached(root, repo, Some(holder.to_path_buf()), None, Some(limits))
    }

    /// The same pool, for its own upkeep, kept within `limits`: what it
    /// holds to tidy is never recorded as used.
    pub fn upkeep(root: &Path, repo: &Path, limits: Option<&yunta_core::PoolConfig>) -> Arc<Self> {
        Self::reached(root, repo, None, None, limits)
    }

    fn reached(
        root: &Path,
        repo: &Path,
        holder: Option<PathBuf>,
        known: Option<PathBuf>,
        limits: Option<&yunta_core::PoolConfig>,
    ) -> Arc<Self> {
        Arc::new(Self {
            root: root.to_path_buf(),
            repo: repo.to_path_buf(),
            holder,
            known,
            limits: limits.cloned().unwrap_or_default(),
            walking: space::Walking::default(),
            located: tokio::sync::OnceCell::new(),
        })
    }

    /// The same pool, reached from `repo` — another checkout of the
    /// project, for a run that moved there — knowing what this handle
    /// knows, and sharing the walks it has under way.
    pub fn reached_from(&self, repo: &Path) -> Arc<Self> {
        Arc::new(Self {
            root: self.root.clone(),
            repo: repo.to_path_buf(),
            holder: self.holder.clone(),
            known: self.known.clone(),
            limits: self.limits.clone(),
            walking: self.walking.clone(),
            located: tokio::sync::OnceCell::new_with(self.located.get().cloned()),
        })
    }

    /// The pool's directory, which exists once the pool has a checkout.
    pub async fn home(&self, supervision: Supervision<'_>) -> Result<PathBuf, WorktreeError> {
        Ok(self.located(supervision).await?.home.clone())
    }

    async fn located(&self, supervision: Supervision<'_>) -> Result<&Located, WorktreeError> {
        self.located
            .get_or_try_init(|| async {
                let recorded = match &self.known {
                    Some(known) => tokio::fs::canonicalize(known).await.ok(),
                    None => None,
                };
                let common_dir = match recorded {
                    Some(recorded) => recorded,
                    None => {
                        let asked = super::common_git_dir(&self.repo, supervision).await?;
                        tokio::fs::canonicalize(&asked).await.unwrap_or(asked)
                    }
                };
                let home = settled(&pool_home(&self.root, &common_dir)).await;
                Ok(Located { home, common_dir })
            })
            .await
    }

    /// Opens `who`'s unit in a checkout of the pool, on `home.base` and on
    /// no branch: the checkout one of its attempts worked in, a free one
    /// put back to that commit, or a new one when none is free. The
    /// checkout's record keeps it for the unit, and the unit's ref keeps
    /// what it commits there.
    pub async fn open(
        self: &Arc<Self>,
        home: UnitHome<'_>,
        who: UnitId,
        attempt: u32,
        supervision: Supervision<'_>,
    ) -> Result<(Unit, Lease), WorktreeError> {
        let keeps = super::UnitRef::of(home.run_id, &who, attempt);
        let refs = super::UnitRef::of_unit(home.run_id, &who);
        let user = fit::User::Unit { refs: &refs };
        let (checkout, lease) = match Box::pin(self.take(user, home.base, supervision)).await? {
            Taken::Held(checkout, lease) => {
                reset(&checkout, None, home.base, supervision).await?;
                (checkout, lease)
            }
            Taken::Nothing { listed } => {
                let fresh = Fresh::Detached { base: home.base };
                self.add(listed, home.repo, fresh, supervision).await?
            }
        };
        lease.keep_for(Some(&keeps)).await;
        let from = head_tree(&checkout, supervision).await?;
        let unit = Unit {
            who,
            worktree: checkout,
            base: home.base.clone(),
            from,
            keeps,
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
        match Box::pin(self.take(fit::User::Passing, base, supervision)).await? {
            Taken::Held(checkout, lease) => {
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
            Taken::Nothing { listed } => {
                let repo = self.repo.clone();
                self.add(listed, &repo, Fresh::Detached { base }, supervision)
                    .await
            }
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

/// Puts `checkout` at `base` — on `branch`, cut anew there, or on none —
/// with nothing of whoever worked there before but what git ignores.
async fn reset(
    checkout: &Path,
    branch: Option<&str>,
    base: &CommitSha,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let mut switch = vec!["switch", "--discard-changes", "-q"];
    match branch {
        Some(branch) => switch.extend(["-C", branch]),
        None => switch.push("--detach"),
    }
    switch.push(base.as_str());
    mutating(checkout, &switch, supervision).await?;
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
