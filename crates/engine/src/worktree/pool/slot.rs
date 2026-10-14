//! One checkout of a project's pool: what holds it, and what it says of
//! its last use.
//!
//! Holding a checkout is a lock file beside it, `slot-N.lock`, taken with
//! the same protocol as every other lock of the engine: it lasts as long as
//! the process, and a gone holder's lock is taken over unless the agents it
//! left still work there. Beside it, `slot-N.held` says which run held it
//! last and when, which is how a takeover asks whether that run's agents
//! still work there, and how the pool tells its recently used checkouts
//! from the rest.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::super::WorktreeError;
use super::Lease;
use crate::lock::{self, Acquired, Contention, LockError, LockOwner, SystemProbe};
use crate::process::Supervision;
use crate::process_registry::{self, EngineLiveness};

/// What every checkout of the pool is called, followed by its number.
pub(super) const SLOT: &str = "slot-";

/// Who held a checkout last, and when — and the unit it is kept for, if
/// one owns it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Held {
    /// The directory of the run that held it.
    pub(super) by: PathBuf,
    pub(super) at: DateTime<Utc>,
    /// The ref of the unit whose checkout this is while its work has not
    /// landed: that unit's attempts take it back first, and nobody else
    /// takes it while what it holds is not in where they start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) unit: Option<String>,
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

/// Which checkout of the pool at `home` `checkout` is: its number, and its
/// path as the file system names it — `None` for a checkout outside the
/// pool.
pub(super) async fn number_of(home: &Path, checkout: &Path) -> Option<(u32, PathBuf)> {
    let path = tokio::fs::canonicalize(checkout)
        .await
        .unwrap_or_else(|_| checkout.to_path_buf());
    if path.parent() != Some(home) {
        return None;
    }
    let number = path
        .file_name()?
        .to_str()?
        .strip_prefix(SLOT)?
        .parse()
        .ok()?;
    Some((number, path))
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
    let Some(owner) = claim(home, number, contention, supervision).await? else {
        return Ok(None);
    };
    if let Some(holder) = holder {
        let now = supervision.clock.now();
        let unit = last_held(home, number).await.and_then(|held| held.unit);
        let held = Held {
            by: holder.to_path_buf(),
            at: now,
            unit,
        };
        record_held(home, number, &held);
    }
    Ok(Some(Lease::of(home, number, owner)))
}

/// Takes the lock of checkout `number` as `acquire` decides it: the record
/// this process wrote, or `None` when the checkout is somebody else's to
/// keep.
async fn claim(
    home: &Path,
    number: u32,
    contention: Contention,
    supervision: Supervision<'_>,
) -> Result<Option<LockOwner>, WorktreeError> {
    let lock_path = home.join(format!("{SLOT}{number}.lock"));
    let keep = |_: &LockOwner| agents_still_work(home, number);
    let taken = lock::acquire(
        &lock_path,
        contention,
        &SystemProbe,
        supervision.clock,
        supervision.cancel,
        &keep,
    )
    .await;
    match taken {
        Ok(Acquired::Fresh(owner)) => Ok(Some(owner)),
        Ok(Acquired::Stolen { owner, dead }) => {
            tracing::info!(
                pid = %dead.pid,
                checkout = number,
                "took over a checkout a process that is gone held"
            );
            Ok(Some(owner))
        }
        Err(
            LockError::Kept { .. }
            | LockError::Held { .. }
            | LockError::Unreadable { .. }
            | LockError::Timeout { .. },
        ) => Ok(None),
        Err(cancelled @ LockError::Cancelled { .. }) => Err(WorktreeError::Lock(cancelled)),
        Err(LockError::Io {
            action,
            lock_path,
            source,
        }) => Err(WorktreeError::Io {
            action: format!("{action} the checkout lock"),
            path: lock_path,
            source,
        }),
    }
}

/// Writes `held` as checkout `number`'s record, under the hold its caller
/// has. A failed write is logged and not raised: what is not recorded only
/// costs the pool a guess.
fn record_held(home: &Path, number: u32, held: &Held) {
    let path = held_path(home, number);
    // blocking: one small record, written whole while the caller holds the checkout.
    if let Err(error) = super::record::write(&path, held) {
        tracing::debug!(%error, path = %path.display(), "a checkout's record went unwritten");
    }
}

/// Records that checkout `number` was used until `at`, by the run at
/// `holder` — or by whoever its record already names, for the pool's own
/// upkeep.
pub(super) async fn used(home: &Path, number: u32, holder: Option<&Path>, at: DateTime<Utc>) {
    let last = last_held(home, number).await;
    let by = match (holder, &last) {
        (Some(holder), _) => holder.to_path_buf(),
        (None, Some(held)) => held.by.clone(),
        (None, None) => return,
    };
    let unit = last.and_then(|held| held.unit);
    record_held(home, number, &Held { by, at, unit });
}

/// Records that checkout `number`, held by its caller, is kept for `unit`
/// — or for nobody: its unit's work landed, or its run let it go.
pub(super) async fn owned(home: &Path, number: u32, unit: Option<&crate::worktree::UnitRef>) {
    let Some(mut held) = last_held(home, number).await else {
        return;
    };
    held.unit = unit.map(|unit| unit.name().to_string());
    record_held(home, number, &held);
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
/// they write where they were started. A registry naming a live engine
/// keeps the checkout, and so does one this binary cannot read. One naming
/// a gone engine keeps it only while one of the run's process groups still
/// answers; no registry at all keeps nothing.
fn agents_still_work(home: &Path, number: u32) -> bool {
    // blocking: one small record, read while the takeover that asks holds
    // the checkout's lock.
    let Some(held) = std::fs::read(held_path(home, number))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Held>(&bytes).ok())
    else {
        return false;
    };
    match process_registry::engine_liveness(&held.by, &SystemProbe) {
        EngineLiveness::Alive | EngineLiveness::Unknown => true,
        EngineLiveness::Unrecorded => false,
        EngineLiveness::Dead => groups_at_work(&held.by),
    }
}

/// Whether one of the process groups the registry under `run_dir` names
/// still answers.
pub(super) fn groups_at_work(run_dir: &Path) -> bool {
    match process_registry::read_registry(run_dir) {
        process_registry::Registry::Read(registry) => {
            !process_registry::running_groups(&registry.doc.process_groups).is_empty()
        }
        _ => false,
    }
}

/// Whether checkout `number`, at `checkout`, may be cleared: no unit keeps
/// it, and its `.git` is missing or names an admin directory that is gone.
pub(super) async fn clearable(home: &Path, number: u32, checkout: &Path) -> bool {
    if last_held(home, number)
        .await
        .is_some_and(|held| held.unit.is_some())
    {
        return false;
    }
    match tokio::fs::read_to_string(checkout.join(".git")).await {
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        Ok(pointer) => match pointer.strip_prefix("gitdir:") {
            Some(admin) => {
                matches!(
                    tokio::fs::try_exists(checkout.join(admin.trim())).await,
                    Ok(false)
                )
            }
            None => false,
        },
    }
}

/// Takes away what is left at `checkout` of a checkout a crash left half
/// added, so its number can be used again, then prunes the git records of
/// the checkouts that are gone. The caller checked `clearable` first.
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
    super::super::prune_worktrees(repo, supervision).await
}
