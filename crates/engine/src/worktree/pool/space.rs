//! What a checkout's build takes on disk: everything git ignores in it,
//! kept beside the checkout as `slot-N.built`. A give-back records where
//! the build is — what git ignores, listed while the checkout is still
//! held — and the walk that counts its bytes runs off the way of whoever
//! gave it back: it takes seconds on a large build. A walk a process did
//! not live to finish is finished by the next one that surveys the pool,
//! or by `gc`, and a checkout kept before builds were recorded is learned
//! by the next take; the pool reads what was last counted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use futures::future::{BoxFuture, FutureExt, Shared};
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
/// none starts a walk another one has under way, and each can wait for
/// one to end.
#[derive(Default, Clone)]
pub(super) struct Walking(std::sync::Arc<std::sync::Mutex<HashMap<PathBuf, Walk>>>);

/// A walk under way, which every caller waiting for it awaits.
type Walk = Shared<BoxFuture<'static, ()>>;

impl Walking {
    /// Counts `built`'s bytes off the caller's way and records them at
    /// `path`, unless a walk of it is under way or a later give-back was
    /// recorded first.
    fn count(&self, path: PathBuf, built: Built) {
        let (walked, ended) = tokio::sync::oneshot::channel::<()>();
        {
            let mut under_way = self.under_way();
            if under_way
                .get(&path)
                .is_some_and(|walk| walk.peek().is_none())
            {
                return;
            }
            under_way.insert(path.clone(), ended.map(|_| ()).boxed().shared());
        }
        let walking = self.clone();
        tokio::spawn(async move {
            let roots = built.roots.clone();
            let bytes = on_disk(roots).await;
            let newer = recorded_at(&path).await.is_some_and(|at| at > built.at);
            if !newer {
                let bytes = Some(bytes);
                record(&path, &Built { bytes, ..built }).await;
            }
            walking.under_way().remove(&path);
            // Whoever waits for the walk is woken by its end.
            drop(walked);
        });
    }

    /// Waits for the walk recording at `path` to end, when one is under
    /// way.
    async fn finished(&self, path: &Path) {
        let walk = self.under_way().get(path).cloned();
        if let Some(walk) = walk {
            walk.await;
        }
    }

    fn under_way(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Walk>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// What checkout `number` of the pool at `home` holds of a build, counted:
/// once a walk of it `walking` has under way ends, or now when nobody
/// counted it. `None` when nothing recorded its build.
pub(super) async fn counted(home: &Path, number: u32, walking: &Walking) -> Option<u64> {
    walking.finished(&built_path(home, number)).await;
    let built = built(home, number).await?;
    Some(match built.bytes {
        Some(bytes) => bytes,
        None => measure_now(home, number, built.roots, built.at).await,
    })
}

/// What `roots` take on disk, counted on a thread that may block.
pub(super) async fn on_disk(roots: Vec<PathBuf>) -> u64 {
    tokio::task::spawn_blocking(move || super::walk::on_disk(&roots))
        .await
        .unwrap_or(0)
}

/// Records that checkout `number` of the pool at `home` was given back at
/// `at` holding `bytes` of build at `roots`, counted.
pub(super) async fn measured(
    home: &Path,
    number: u32,
    roots: Vec<PathBuf>,
    bytes: u64,
    at: DateTime<Utc>,
) {
    let built = Built {
        roots,
        bytes: Some(bytes),
        at,
    };
    record(&built_path(home, number), &built).await;
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
    let bytes = on_disk(roots.clone()).await;
    measured(home, number, roots, bytes, at).await;
    bytes
}

/// Records that checkout `number` of the pool at `home` holds no build
/// since `at`.
pub(super) async fn emptied(home: &Path, number: u32, at: DateTime<Utc>) {
    let built = Built {
        roots: Vec::new(),
        bytes: Some(0),
        at,
    };
    record(&built_path(home, number), &built).await;
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
