//! One checkout of a project's pool: what holds it, and what it says of
//! its last use.
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

use super::super::WorktreeError;
use super::Lease;
use crate::lock::{self, Acquired, Contention, LockError, SystemProbe};
use crate::process::Supervision;

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
    let lock_path = home.join(format!("{SLOT}{number}.lock"));
    let lease = match lock::acquire(&lock_path, contention, &SystemProbe, supervision.clock).await {
        Ok(Acquired::Fresh) => Lease::of(home, number),
        Ok(Acquired::Stolen { dead }) => {
            let lease = Lease::of(home, number);
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
        let unit = last_held(home, number).await.and_then(|held| held.unit);
        let held = Held {
            by: holder.to_path_buf(),
            at: now,
            unit,
        };
        record(&held_path(home, number), &held).await;
    }
    Ok(Some(lease))
}

/// Writes `value` at `path` whole — through a file beside it, renamed
/// into place, so a reader never meets half of it. What is not recorded
/// only costs the pool a guess.
async fn record(path: &Path, value: &impl Serialize) {
    let Ok(json) = serde_json::to_vec(value) else {
        return;
    };
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let written = match tokio::fs::write(&partial, json).await {
        Ok(()) => tokio::fs::rename(&partial, path).await,
        Err(error) => Err(error),
    };
    if let Err(error) = written {
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
    record(&held_path(home, number), &Held { by, at, unit }).await;
}

/// Records that checkout `number`, held by its caller, is kept for `unit`
/// — or for nobody: its unit's work landed, or its run let it go.
pub(super) async fn owned(home: &Path, number: u32, unit: Option<&crate::worktree::UnitRef>) {
    let Some(mut held) = last_held(home, number).await else {
        return;
    };
    held.unit = unit.map(|unit| unit.name().to_string());
    record(&held_path(home, number), &held).await;
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
