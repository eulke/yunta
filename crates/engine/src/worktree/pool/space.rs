//! What a checkout's build takes on disk: everything git ignores in it,
//! kept beside the checkout as `slot-N.built`. A give-back records where
//! the build is — what git ignores, listed while the checkout is still
//! held — and the walk that counts its bytes runs off the way of whoever
//! gave it back: it takes seconds on a large build. The walk records its
//! count only under a hold it takes without waiting, so a checkout somebody
//! holds keeps the build its holder records. A walk a process did not live
//! to finish is finished by the next one that surveys the pool, or by `gc`,
//! and a checkout kept before builds were recorded is learned by the next
//! take; the pool reads what was last counted.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use yunta_core::Clock;

use super::slot::{self, SLOT};
use crate::lock::Contention;
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

/// What a build takes on disk, as far as anybody counted it. Ordered by
/// weight: a build nobody counted sorts after every counted one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Size {
    /// The bytes its roots take on disk.
    Counted(u64),
    /// Nobody counted them, or they cannot be counted.
    Uncounted,
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

    /// What its build takes, as far as anybody counted it.
    pub(super) fn size(&self) -> Size {
        self.bytes.map_or(Size::Uncounted, Size::Counted)
    }

    /// What its build adds to a sum: nothing for a build nobody counted.
    pub(super) fn counted(&self) -> u64 {
        match self.size() {
            Size::Counted(bytes) => bytes,
            Size::Uncounted => 0,
        }
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
/// off the caller's way — unless a walk of it is under way already. The
/// caller holds the checkout, and the record is written under that hold.
pub(super) fn given_back(
    home: &Path,
    number: u32,
    roots: Vec<PathBuf>,
    at: DateTime<Utc>,
    walking: &Walking,
) -> Built {
    let bytes = roots.is_empty().then_some(0);
    let built = Built { roots, bytes, at };
    // blocking: one small record, written whole while the caller holds the checkout.
    record(&built_path(home, number), &built);
    if built.bytes.is_none() {
        walking.count(home.to_path_buf(), number, built.clone());
    }
    built
}

/// Learns the build of every checkout of `slots` that has no record of
/// one — added before builds were recorded — from what git ignores in it,
/// listed once and recorded as of its last use, its bytes counted off the
/// caller's way. Until then its next user could not tell a warm checkout
/// from an empty one. A checkout somebody holds is left as it is: its
/// holder records its build.
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
        let held = slot::hold(home, slot.number, Contention::Refuse, None, supervision);
        let lease = match held.await {
            Ok(Some(lease)) => lease,
            Ok(None) => continue,
            Err(error) => {
                tracing::debug!(
                    %error,
                    checkout = slot.number,
                    "a checkout's build was not learned"
                );
                continue;
            }
        };
        let Some(roots) = ignored(&slot.path, supervision).await else {
            continue;
        };
        let used = slot.held.as_ref().map_or(now, |held| held.at);
        slot.built = Some(given_back(home, slot.number, roots, used, walking));
        drop(lease);
    }
}

/// Counts every build the survey `slots` found recorded and not counted,
/// off the caller's way.
pub(super) fn count_pending(home: &Path, slots: &[super::survey::Slot], walking: &Walking) {
    for slot in slots {
        if let Some(built) = slot.built.as_ref().filter(|built| built.bytes.is_none()) {
            walking.count(home.to_path_buf(), slot.number, built.clone());
        }
    }
}

/// The walks a process has under way, by the record each one finishes:
/// shared by every handle on the pool the process reaches it through, so
/// none starts a walk another one has under way. It holds the clock too,
/// because a spawned task needs `'static` data and cannot borrow one.
#[derive(Clone)]
pub(super) struct Walking {
    under_way: Arc<Mutex<HashSet<PathBuf>>>,
    clock: Arc<dyn Clock>,
}

impl Walking {
    /// No walk under way, and the clock its holds are taken by.
    pub(super) fn new(clock: Arc<dyn Clock>) -> Self {
        Walking {
            under_way: Arc::default(),
            clock,
        }
    }

    /// Counts `built`'s bytes off the caller's way and records them as the
    /// build of checkout `number` of the pool at `home`, unless a walk of it
    /// is under way.
    fn count(&self, home: PathBuf, number: u32, built: Built) {
        let path = built_path(&home, number);
        let started = self
            .under_way
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(path.clone());
        if !started {
            return;
        }
        let under_way = self.under_way.clone();
        let clock = self.clock.clone();
        tokio::spawn(async move {
            count_and_record(&home, number, &path, built, &*clock).await;
            under_way
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&path);
        });
    }
}

/// Walks `built`'s roots on a blocking thread and records the bytes as the
/// build of checkout `number`, under a hold taken without waiting: a
/// checkout somebody holds keeps the build its holder records, and a count
/// that finds a newer record leaves that record standing.
async fn count_and_record(home: &Path, number: u32, path: &Path, built: Built, clock: &dyn Clock) {
    let roots = built.roots.clone();
    let walked = tokio::task::spawn_blocking(move || super::walk::on_disk(&roots)).await;
    let Ok(Some(bytes)) = walked else {
        return;
    };
    let cancel = CancellationToken::new();
    let supervision = Supervision::outside_any_run(&cancel, clock);
    match slot::hold(home, number, Contention::Refuse, None, supervision).await {
        Ok(Some(_lease)) => {
            let newer = recorded_at(path).await.is_some_and(|at| at > built.at);
            if !newer {
                let counted = Built {
                    bytes: Some(bytes),
                    ..built
                };
                // blocking: one small record, written whole under the hold.
                record(path, &counted);
            }
        }
        // Held by somebody else: its holder records what it leaves.
        Ok(None) => {}
        Err(error) => {
            tracing::debug!(%error, checkout = number, "a checkout's build went uncounted");
        }
    }
}

/// Counts `roots` — what git ignores in checkout `number` of the pool at
/// `home` — and records it as of `at`, waiting for the walk. Answers the
/// bytes, or `None` when they cannot be counted, and then nothing is
/// recorded. The caller holds the checkout.
pub(super) async fn measure_now(
    home: &Path,
    number: u32,
    roots: Vec<PathBuf>,
    at: DateTime<Utc>,
) -> Option<u64> {
    let walked = roots.clone();
    let bytes = tokio::task::spawn_blocking(move || super::walk::on_disk(&walked))
        .await
        .ok()??;
    let built = Built {
        roots,
        bytes: Some(bytes),
        at,
    };
    // blocking: one small record, written whole while the caller holds the checkout.
    record(&built_path(home, number), &built);
    Some(bytes)
}

/// Records that checkout `number` of the pool at `home` holds no build
/// since `at`. The caller holds the checkout.
pub(super) fn emptied(home: &Path, number: u32, at: DateTime<Utc>) {
    // blocking: one small record, written whole while the caller holds the checkout.
    record(&built_path(home, number), &Built::empty(at));
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

/// Writes `built` at `path` under the caller's hold. A failed write is
/// logged and not raised: the record stays as it was.
fn record(path: &Path, built: &Built) {
    if let Err(error) = super::record::write(path, built) {
        tracing::debug!(%error, path = %path.display(), "a checkout's build went unrecorded");
    }
}

fn built_path(home: &Path, number: u32) -> PathBuf {
    home.join(format!("{SLOT}{number}.built"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_unmeasurable_build_records_nothing() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let locked = home.path().join("locked-build");
        tokio::fs::create_dir(&locked).await.unwrap();
        tokio::fs::set_permissions(&locked, Permissions::from_mode(0o000))
            .await
            .unwrap();
        let measured = measure_now(home.path(), 1, vec![locked.clone()], stamp()).await;
        // Readable again before any assertion, so the temporary directory can be removed.
        tokio::fs::set_permissions(&locked, Permissions::from_mode(0o755))
            .await
            .unwrap();
        assert_eq!(
            measured, None,
            "a directory that cannot be read has no size"
        );
        assert!(
            !built_path(home.path(), 1).exists(),
            "an unmeasured build was recorded"
        );
    }

    #[test]
    fn an_uncounted_build_is_heaviest_and_adds_nothing() {
        let built = Built {
            roots: vec![],
            bytes: None,
            at: stamp(),
        };
        assert_eq!(built.size(), Size::Uncounted);
        assert!(
            Size::Counted(u64::MAX) < built.size(),
            "an uncounted build must rank heaviest"
        );
        assert_eq!(
            built.counted(),
            0,
            "an uncounted build must add nothing to a sum"
        );
    }
}
