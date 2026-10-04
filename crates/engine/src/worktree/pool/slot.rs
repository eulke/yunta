//! One checkout of a project's pool: what holds it, what it holds, and
//! how near it stands to where its next user starts.
//!
//! Holding a checkout is a lock file beside it, `slot-N.lock`, taken with
//! the same protocol as every other lock of the engine: it lasts as long as
//! the process, and a gone holder's lock is taken over. Beside it,
//! `slot-N.held` says which run held it last and when, which is how a
//! takeover asks whether that run's agents still work there, and how the
//! pool tells its recently used checkouts from the rest.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use yunta_core::process::signal::Liveness;
use yunta_core::CommitSha;

use super::super::WorktreeError;
use super::Lease;
use crate::lock::{self, Acquired, Contention, LockError, SystemProbe};
use crate::process::Supervision;

/// What every checkout of the pool is called, followed by its number.
pub(super) const SLOT: &str = "slot-";

/// Who held a checkout last, and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Held {
    /// The directory of the run that held it.
    pub(super) by: PathBuf,
    pub(super) at: DateTime<Utc>,
}

/// The checkouts under `home`, by number.
pub(super) async fn listed(home: &Path) -> Vec<(u32, PathBuf)> {
    let Ok(mut entries) = tokio::fs::read_dir(home).await else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(number) = name.strip_prefix(SLOT).and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let path = entry.path();
        if !tokio::fs::metadata(&path)
            .await
            .is_ok_and(|meta| meta.is_dir())
        {
            continue;
        }
        let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);
        slots.push((number, path));
    }
    slots.sort();
    slots
}

/// How fitting `checkout` is for a user that owns the branches starting
/// with `owner` (none, for a user that works on no branch) and starts
/// from `target`: lower is better, `None` when it is not this user's to
/// take. Only a clean checkout is ever handed out: what is uncommitted in
/// one is somebody's. One on its owner's branch goes back to its owner
/// first. One on another branch is that branch's owner's while its work
/// is not in `target`; once it is — landed where the user starts — it is
/// anybody's, and keeps its branch until somebody takes it, so its owner
/// can still pick its session back up there. One on no branch is
/// anybody's. Among those, the nearer to `target` the better.
pub(super) async fn rank(
    checkout: &Path,
    owner: Option<&str>,
    target: &CommitSha,
    supervision: Supervision<'_>,
) -> Option<(u8, usize)> {
    let branch = crate::git::output(checkout, &["branch", "--show-current"], supervision)
        .await
        .ok()?;
    if !is_clean(checkout, supervision).await {
        return None;
    }
    match (branch.trim(), owner) {
        ("", _) => Some((1, distance(checkout, target, supervision).await)),
        (on, Some(owner)) if on.starts_with(owner) => Some((0, 0)),
        _ if landed_in(checkout, target, supervision).await => {
            Some((1, distance(checkout, target, supervision).await))
        }
        _ => None,
    }
}

/// Whether everything `checkout` committed is in `target`.
async fn landed_in(checkout: &Path, target: &CommitSha, supervision: Supervision<'_>) -> bool {
    crate::git::success(
        checkout,
        &["merge-base", "--is-ancestor", "HEAD", target.as_str()],
        supervision,
    )
    .await
    .unwrap_or(false)
}

/// Whether nothing in `checkout` is uncommitted beyond the request a
/// session writes for the engine to read.
pub(super) async fn is_clean(checkout: &Path, supervision: Supervision<'_>) -> bool {
    let request = format!(
        ":(exclude){}",
        crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE
    );
    crate::git::output(
        checkout,
        &["status", "--porcelain", "--", ".", request.as_str()],
        supervision,
    )
    .await
    .is_ok_and(|status| status.trim().is_empty())
}

/// How many paths differ between what `checkout` holds and `target`: what
/// a build there has to redo.
async fn distance(checkout: &Path, target: &CommitSha, supervision: Supervision<'_>) -> usize {
    crate::git::output(
        checkout,
        &["diff", "--name-only", "HEAD", target.as_str()],
        supervision,
    )
    .await
    .map_or(usize::MAX, |names| names.lines().count())
}

/// Whether git sees a working tree at `checkout`: one a crash left half
/// added does not.
pub(super) async fn is_checkout(checkout: &Path, supervision: Supervision<'_>) -> bool {
    crate::git::success(
        checkout,
        &["rev-parse", "--is-inside-work-tree"],
        supervision,
    )
    .await
    .unwrap_or(false)
}

/// Holds checkout `number` of the pool at `home` — for the run at `holder`,
/// or for the pool's own upkeep when there is none — or answers `None` when
/// somebody else holds it: a live process, or a gone one whose run's
/// agents still work there. A run's hold is recorded; upkeep's is not, so
/// it never makes a checkout look recently used.
pub(super) async fn hold(
    home: &Path,
    number: u32,
    contention: Contention,
    holder: Option<&Path>,
    supervision: Supervision<'_>,
) -> Result<Option<Lease>, WorktreeError> {
    let lock_path = home.join(format!("{SLOT}{number}.lock"));
    let lease = match lock::acquire(&lock_path, contention, &SystemProbe, supervision.clock).await {
        Ok(Acquired::Fresh) => Lease::of(lock_path),
        Ok(Acquired::Stolen { dead }) => {
            let lease = Lease::of(lock_path);
            if agents_still_work(home, number).await {
                return Ok(None);
            }
            tracing::info!(
                pid = %dead.pid,
                checkout = number,
                "took over a checkout a process that is gone held"
            );
            lease
        }
        Err(LockError::Held { .. } | LockError::Unreadable { .. } | LockError::Timeout { .. }) => {
            return Ok(None);
        }
        Err(LockError::Io {
            action,
            lock_path,
            source,
        }) => {
            return Err(WorktreeError::Io {
                action: format!("{action} the checkout lock"),
                path: lock_path,
                source,
            });
        }
    };
    if let Some(holder) = holder {
        let now = supervision.clock.now();
        let held = Held {
            by: holder.to_path_buf(),
            at: now,
        };
        record(&held_path(home, number), &held).await;
        record_busy(home, now).await;
    }
    Ok(Some(lease))
}

/// How long the pool remembers how many checkouts were busy at once.
pub(super) fn remembered() -> chrono::Duration {
    chrono::Duration::days(30)
}

/// The most checkouts the pool saw busy at once, and when it last saw that
/// many — or more, after what it remembers ran out.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Busiest {
    count: usize,
    at: DateTime<Utc>,
}

/// The most checkouts busy at once, and when, as the pool last recorded it.
pub(super) async fn busiest(home: &Path) -> Option<(usize, DateTime<Utc>)> {
    let bytes = tokio::fs::read(home.join("busiest")).await.ok()?;
    let busiest: Busiest = serde_json::from_slice(&bytes).ok()?;
    Some((busiest.count, busiest.at))
}

/// Records how many checkouts are busy now, when that is the most the
/// pool remembers.
async fn record_busy(home: &Path, now: DateTime<Utc>) {
    let mut busy = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(home).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            busy += usize::from(name.starts_with(SLOT) && name.ends_with(".lock"));
        }
    }
    let higher = match busiest(home).await {
        Some((count, at)) => busy > count || now - at > remembered(),
        None => true,
    };
    if higher {
        record(
            &home.join("busiest"),
            &Busiest {
                count: busy,
                at: now,
            },
        )
        .await;
    }
}

/// Writes `value` at `path`; what is not recorded only costs the pool a
/// guess.
async fn record(path: &Path, value: &impl Serialize) {
    let Ok(json) = serde_json::to_vec(value) else {
        return;
    };
    if let Err(error) = tokio::fs::write(path, json).await {
        tracing::debug!(%error, path = %path.display(), "a checkout's record went unwritten");
    }
}

/// Who held checkout `number` last, when it says.
pub(super) async fn last_held(home: &Path, number: u32) -> Option<Held> {
    let bytes = tokio::fs::read(held_path(home, number)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn held_path(home: &Path, number: u32) -> PathBuf {
    home.join(format!("{SLOT}{number}.held"))
}

/// Whether the run that held checkout `number` last still has agents at
/// work: an engine killed outright leaves its sessions running, and
/// they write where they were started.
async fn agents_still_work(home: &Path, number: u32) -> bool {
    let Some(held) = last_held(home, number).await else {
        return false;
    };
    match crate::process_registry::read_registry(&held.by) {
        crate::process_registry::Registry::Read(registry) => registry
            .doc
            .process_groups
            .iter()
            .any(|group| yunta_core::process::signal::liveness(*group) == Liveness::Alive),
        _ => false,
    }
}

/// Takes away what is left at `checkout` of a checkout a crash left half
/// added, so its number can be used again.
pub(super) async fn clear(
    repo: &Path,
    checkout: &Path,
    supervision: Supervision<'_>,
) -> Result<(), WorktreeError> {
    match tokio::fs::remove_dir_all(checkout).await {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
            return Err(WorktreeError::Io {
                action: "clear a checkout left half added".to_string(),
                path: checkout.to_path_buf(),
                source,
            });
        }
        _ => {}
    }
    let common_dir = super::super::common_git_dir(repo, supervision).await?;
    let _mutation_lock =
        super::super::lock_worktree_mutations(&common_dir, supervision.clock).await?;
    super::super::run_git(repo, &["worktree", "prune"], supervision).await?;
    Ok(())
}
