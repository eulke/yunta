//! What a checkout's build takes on disk: everything git ignores in it,
//! kept beside the checkout as `slot-N.built`. A give-back records where
//! the build is — what git ignores, listed while the checkout is still
//! held — and the walk that counts its bytes runs off the way of whoever
//! gave it back: it takes seconds on a large build. A walk a process did
//! not live to finish is finished by the next one that surveys the pool,
//! or by `gc`, and a checkout kept before builds were recorded is learned
//! by the next take; the pool reads what was last counted.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::slot::SLOT;
use crate::process::Supervision;

/// The build a checkout was last given back with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Built {
    /// What git ignores in the checkout — where its build is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) roots: Vec<PathBuf>,
    /// What those take on disk — `None` while nobody finished counting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) bytes: Option<u64>,
    /// When the checkout was given back with it.
    pub(super) at: DateTime<Utc>,
}

impl Built {
    /// No build, as of `at`.
    pub(super) fn empty(at: DateTime<Utc>) -> Built {
        Built {
            roots: Vec::new(),
            bytes: Some(0),
            at,
        }
    }

    /// Whether the checkout holds a build at all.
    pub(super) fn holds(&self) -> bool {
        !self.roots.is_empty() || self.bytes.is_some_and(|bytes| bytes > 0)
    }

    /// What its build takes, as far as anybody counted.
    pub(super) fn counted(&self) -> u64 {
        self.bytes.unwrap_or(0)
    }
}

/// The build checkout `number` of the pool at `home` was last given back
/// with.
pub(super) async fn built(home: &Path, number: u32) -> Option<Built> {
    let bytes = tokio::fs::read(built_path(home, number)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Records that checkout `number` of the pool at `home` was given back at
/// `at` holding what git ignores at `roots`, and counts what that takes
/// off the caller's way — unless `walking`, the walks this process has
/// under way, already counts it.
pub(super) async fn given_back(
    home: &Path,
    number: u32,
    roots: Vec<PathBuf>,
    at: DateTime<Utc>,
    walking: &Walking,
) -> Built {
    let bytes = roots.is_empty().then_some(0);
    let built = Built { roots, bytes, at };
    record(&built_path(home, number), &built).await;
    if built.bytes.is_none() {
        walking.count(built_path(home, number), built.clone());
    }
    built
}

/// Learns the build of every checkout of `slots` that has no record of
/// one — added before builds were recorded — from what git ignores in it,
/// listed once and recorded as of its last use, its bytes counted off the
/// caller's way. Until then its next user could not tell a warm checkout
/// from an empty one.
pub(super) async fn learn(
    home: &Path,
    slots: &mut [super::survey::Slot],
    walking: &Walking,
    supervision: Supervision<'_>,
) {
    let now = supervision.clock.now();
    for slot in slots.iter_mut() {
        if slot.built.is_some() || slot.standing.is_none() {
            continue;
        }
        let Some(roots) = ignored(&slot.path, supervision).await else {
            continue;
        };
        let used = slot.held.as_ref().map_or(now, |held| held.at);
        slot.built = Some(given_back(home, slot.number, roots, used, walking).await);
    }
}

/// Counts every build the survey `slots` found recorded and not counted,
/// off the caller's way.
pub(super) fn count_pending(home: &Path, slots: &[super::survey::Slot], walking: &Walking) {
    for slot in slots {
        if let Some(built) = slot.built.as_ref().filter(|built| built.bytes.is_none()) {
            walking.count(built_path(home, slot.number), built.clone());
        }
    }
}

/// The walks a process has under way, by the record each one finishes:
/// shared by every handle on the pool the process reaches it through, so
/// none starts a walk another one has under way.
#[derive(Default, Clone)]
pub(super) struct Walking(std::sync::Arc<std::sync::Mutex<std::collections::HashSet<PathBuf>>>);

impl Walking {
    /// Counts `built`'s bytes off the caller's way and records them at
    /// `path`, unless a walk of it is under way or a later give-back was
    /// recorded first.
    fn count(&self, path: PathBuf, built: Built) {
        let under_way = self.0.clone();
        let started = under_way
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(path.clone());
        if !started {
            return;
        }
        tokio::spawn(async move {
            let roots = built.roots.clone();
            let counted = tokio::task::spawn_blocking(move || super::walk::on_disk(&roots)).await;
            if let Ok(bytes) = counted {
                let newer = recorded_at(&path).await.is_some_and(|at| at > built.at);
                if !newer {
                    let bytes = Some(bytes);
                    record(&path, &Built { bytes, ..built }).await;
                }
            }
            under_way
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&path);
        });
    }
}

/// Counts `roots` — what git ignores in checkout `number` of the pool at
/// `home` — and records it as of `at`, waiting for the walk: what `gc`
/// does where nobody counted. Answers the bytes.
pub(super) async fn measure_now(
    home: &Path,
    number: u32,
    roots: Vec<PathBuf>,
    at: DateTime<Utc>,
) -> u64 {
    let walked = roots.clone();
    let bytes = tokio::task::spawn_blocking(move || super::walk::on_disk(&walked))
        .await
        .unwrap_or(0);
    let built = Built {
        roots,
        bytes: Some(bytes),
        at,
    };
    record(&built_path(home, number), &built).await;
    bytes
}

/// Records that checkout `number` of the pool at `home` holds no build
/// since `at`.
pub(super) async fn emptied(home: &Path, number: u32, at: DateTime<Utc>) {
    record(&built_path(home, number), &Built::empty(at)).await;
}

/// The paths git ignores in `checkout`, whole directories named once —
/// `None` when git could not say.
pub(super) async fn ignored(checkout: &Path, supervision: Supervision<'_>) -> Option<Vec<PathBuf>> {
    let ignored = [
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "--directory",
        "--no-empty-directory",
    ];
    let listed = crate::git::output(checkout, &ignored, supervision)
        .await
        .ok()?;
    Some(
        listed
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| checkout.join(line.trim_end_matches('/')))
            .collect(),
    )
}

async fn recorded_at(path: &Path) -> Option<DateTime<Utc>> {
    let bytes = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice::<Built>(&bytes)
        .ok()
        .map(|built| built.at)
}

/// Writes `built` at `path` whole: through a file beside it, renamed into
/// place, so a reader never meets half of it.
async fn record(path: &Path, built: &Built) {
    let Ok(json) = serde_json::to_vec(built) else {
        return;
    };
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let written = match tokio::fs::write(&partial, json).await {
        Ok(()) => tokio::fs::rename(&partial, path).await,
        Err(error) => Err(error),
    };
    if let Err(error) = written {
        tracing::debug!(%error, path = %path.display(), "a checkout's build went unrecorded");
    }
}

fn built_path(home: &Path, number: u32) -> PathBuf {
    home.join(format!("{SLOT}{number}.built"))
}
