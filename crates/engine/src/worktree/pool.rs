//! The checkouts a run's units work in, reused from one unit to the next.
//!
//! A unit of work used to open a checkout of its own and leave it behind:
//! a loop of ten tasks made ten checkouts, and a project whose criteria
//! build something built it from nothing ten times. A run instead keeps a
//! pool of checkouts — as many as it ever has units at work at once — and
//! a unit takes a free one, put back to the commit the unit starts from on
//! a branch of its own. What git ignores — a build's output — stays where
//! the last unit left it, so the next build starts warm.
//!
//! A checkout is free when no unit holds it and nothing in it is anybody's
//! still: its work landed on the run's tree, or it never made any, and
//! nothing in it is uncommitted. A blocked task's committed work, or a
//! node's uncommitted work a session may continue on, keeps its checkout
//! out of the pool until the run is done with it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use super::{head_tree, prepare_worktree, Unit, UnitHome, UnitId, WorktreeError};
use crate::process::Supervision;

/// What every checkout of the pool is called, followed by its number.
const SLOT: &str = "slot-";

/// A run's checkouts, for one invocation.
pub struct CheckoutPool {
    /// Where the checkouts live.
    home: PathBuf,
    /// The checkouts a unit holds this moment.
    held: Mutex<BTreeSet<PathBuf>>,
    /// One unit chooses at a time, so two never choose the same checkout.
    choosing: tokio::sync::Mutex<()>,
}

/// A unit's hold on a checkout of the pool, given back when dropped.
pub struct Lease {
    pool: Arc<CheckoutPool>,
    checkout: PathBuf,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.pool
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.checkout);
    }
}

impl CheckoutPool {
    /// The pool of the run whose directory is `run_dir`.
    pub fn new(run_dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            home: crate::run_dir::unit_worktrees(run_dir),
            held: Mutex::new(BTreeSet::new()),
            choosing: tokio::sync::Mutex::new(()),
        })
    }

    /// Opens `who`'s unit in a checkout of the pool, on a branch of its own
    /// cut from `home.base`: a free checkout put back to that commit, or a
    /// new one when none is free.
    pub async fn open(
        self: &Arc<Self>,
        home: UnitHome<'_>,
        who: UnitId,
        attempt: u32,
        supervision: Supervision<'_>,
    ) -> Result<(Unit, Lease), WorktreeError> {
        let branch = super::unit_branch(home.run_id, &who, attempt);
        let _turn = self.choosing.lock().await;
        let checkout = match self.free(home.base, &branch, supervision).await? {
            Some(checkout) => {
                reset(&checkout, &branch, home.base, supervision).await?;
                checkout
            }
            None => {
                let checkout = self.next_slot().await;
                prepare_worktree(
                    home.repo,
                    &checkout,
                    home.base,
                    &branch,
                    yunta_core::Isolation::Worktree,
                    supervision,
                )
                .await?;
                tokio::fs::canonicalize(&checkout).await.unwrap_or(checkout)
            }
        };
        let lease = self.hold_canonical(checkout.clone());
        let from = head_tree(&checkout, supervision).await?;
        let unit = Unit {
            who,
            worktree: checkout,
            base: home.base.clone(),
            from,
        };
        Ok((unit, lease))
    }

    /// A checkout of the pool on `base` with no branch of its own — what
    /// a probe of the run's tree runs in — held for its caller. A free
    /// one is put back to `base`; a new one is added when none is free.
    pub async fn open_detached(
        self: &Arc<Self>,
        repo: &Path,
        base: &yunta_core::CommitSha,
        supervision: Supervision<'_>,
    ) -> Result<(PathBuf, Lease), WorktreeError> {
        let _turn = self.choosing.lock().await;
        let checkout = match self.free(base, "", supervision).await? {
            Some(checkout) => {
                crate::git::output(
                    &checkout,
                    &[
                        "switch",
                        "--discard-changes",
                        "-q",
                        "--detach",
                        base.as_str(),
                    ],
                    supervision,
                )
                .await?;
                crate::git::output(&checkout, &["clean", "-q", "-ffd"], supervision).await?;
                checkout
            }
            None => {
                let checkout = self.next_slot().await;
                add_detached(repo, &checkout, base, supervision).await?;
                tokio::fs::canonicalize(&checkout).await.unwrap_or(checkout)
            }
        };
        let lease = self.hold_canonical(checkout.clone());
        Ok((checkout, lease))
    }

    /// Holds `checkout` for a unit that reopened it, so no other unit is
    /// given it while that unit works there. Held by its canonical path,
    /// which is how git names a checkout and how the pool finds its own.
    pub async fn hold(self: &Arc<Self>, checkout: PathBuf) -> Lease {
        let checkout = tokio::fs::canonicalize(&checkout).await.unwrap_or(checkout);
        self.hold_canonical(checkout)
    }

    /// Holds `checkout`, already named canonically.
    fn hold_canonical(self: &Arc<Self>, checkout: PathBuf) -> Lease {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(checkout.clone());
        Lease {
            pool: self.clone(),
            checkout,
        }
    }

    /// The checkouts of the pool on disk, by number.
    async fn slots(&self) -> Vec<(u32, PathBuf)> {
        let Ok(mut entries) = tokio::fs::read_dir(&self.home).await else {
            return Vec::new();
        };
        let mut slots = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(number) = name.strip_prefix(SLOT).and_then(|n| n.parse::<u32>().ok()) {
                let path = entry.path();
                let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);
                slots.push((number, path));
            }
        }
        slots.sort();
        slots
    }

    /// The checkout a unit on `branch` takes: the one already on that
    /// branch when it is free, or else the lowest free one.
    async fn free(
        &self,
        base: &yunta_core::CommitSha,
        branch: &str,
        supervision: Supervision<'_>,
    ) -> Result<Option<PathBuf>, WorktreeError> {
        let held = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let mut candidates: Vec<PathBuf> = self
            .slots()
            .await
            .into_iter()
            .map(|(_, path)| path)
            .filter(|path| !held.contains(path))
            .collect();
        let named = (!branch.is_empty()).then_some(branch);
        if let Some(on_branch) = match named {
            Some(branch) => on_branch(&candidates, branch, supervision).await,
            None => None,
        } {
            candidates.retain(|path| path != &on_branch);
            candidates.insert(0, on_branch);
        }
        for candidate in candidates {
            if is_free(&candidate, base, supervision).await {
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }

    /// Where the pool's next new checkout goes.
    async fn next_slot(&self) -> PathBuf {
        let next = self
            .slots()
            .await
            .last()
            .map_or(1, |(number, _)| number + 1);
        self.home.join(format!("{SLOT}{next}"))
    }
}

/// Puts `checkout` back to the commit it is on, with nothing a probe
/// wrote left in it but what git ignores — so it is free again.
pub async fn put_back(checkout: &Path, supervision: Supervision<'_>) -> Result<(), WorktreeError> {
    crate::git::output(checkout, &["reset", "-q", "--hard"], supervision).await?;
    crate::git::output(checkout, &["clean", "-q", "-ffd"], supervision).await?;
    Ok(())
}

/// Adds a checkout of `repo` at `checkout`, on `base` with no branch.
async fn add_detached(
    repo: &Path,
    checkout: &Path,
    base: &yunta_core::CommitSha,
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
    super::run_git(
        repo,
        &["worktree", "add", "--detach", &path, base.as_str()],
        supervision,
    )
    .await?;
    Ok(())
}

/// The candidate already on `branch`, when one is.
async fn on_branch(
    candidates: &[PathBuf],
    branch: &str,
    supervision: Supervision<'_>,
) -> Option<PathBuf> {
    for candidate in candidates {
        let on = crate::git::output(candidate, &["branch", "--show-current"], supervision).await;
        if on.is_ok_and(|on| on.trim() == branch) {
            return Some(candidate.clone());
        }
    }
    None
}

/// Whether nothing in `checkout` is anybody's still: whatever it
/// committed is in `base` — landed on the run's tree, or never made — and
/// nothing in it is uncommitted beyond what a unit writes for the engine
/// to read.
async fn is_free(
    checkout: &Path,
    base: &yunta_core::CommitSha,
    supervision: Supervision<'_>,
) -> bool {
    let request = format!(
        ":(exclude){}",
        crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    );
    let clean = crate::git::output(
        checkout,
        &["status", "--porcelain", "--", ".", request.as_str()],
        supervision,
    )
    .await
    .is_ok_and(|status| status.trim().is_empty());
    clean
        && crate::git::success(
            checkout,
            &["merge-base", "--is-ancestor", "HEAD", base.as_str()],
            supervision,
        )
        .await
        .unwrap_or(false)
}

/// Puts `checkout` on a new `branch` at `base`, with nothing of the unit
/// that worked there before but what git ignores.
async fn reset(
    checkout: &Path,
    branch: &str,
    base: &yunta_core::CommitSha,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    crate::git::output(
        checkout,
        &[
            "switch",
            "--discard-changes",
            "-q",
            "-C",
            branch,
            base.as_str(),
        ],
        supervision,
    )
    .await?;
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

/// Takes away every checkout the run's units worked in under `run_dir`,
/// and every branch of theirs whose work is in what `repo` — the run's
/// tree as it closes — stands on. A branch holding work that never landed
/// — a blocked task's — stays for whoever wants it, until the run is
/// collected.
pub async fn release_unit_checkouts(
    repo: &Path,
    run_dir: &Path,
    run_id: &yunta_core::RunId,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    let landed = super::head_commit(repo, supervision).await?;
    let home = crate::run_dir::unit_worktrees(run_dir);
    let home = tokio::fs::canonicalize(&home).await.unwrap_or(home);
    let listing = super::run_git(repo, &["worktree", "list", "--porcelain"], supervision).await?;
    let common_dir = super::common_git_dir(repo, supervision).await?;
    let _mutation_lock = super::lock_worktree_mutations(&common_dir, supervision.clock).await?;
    for (checkout, _) in super::checkouts(&listing) {
        if checkout.starts_with(&home) {
            let path = checkout.display().to_string();
            super::run_git(repo, &["worktree", "remove", "--force", &path], supervision).await?;
        }
    }
    let ours = format!("refs/heads/{}/", super::run_units(run_id));
    let branches = super::run_git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", &ours],
        supervision,
    )
    .await?;
    for branch in branches.lines().map(str::trim).filter(|b| !b.is_empty()) {
        if crate::git::success(
            repo,
            &["merge-base", "--is-ancestor", branch, landed.as_str()],
            supervision,
        )
        .await?
        {
            super::run_git(repo, &["branch", "-D", branch], supervision).await?;
        }
    }
    Ok(())
}
