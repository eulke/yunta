//! What a checkout's build takes on disk: everything git ignores in it,
//! measured when the checkout is given back and kept beside it as
//! `slot-N.built`. The measurement walks files, which takes seconds on a
//! large build, so it runs out of the way of the run that gave the
//! checkout back; the pool reads what it last found.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::slot::SLOT;
use crate::process::Supervision;

/// What a checkout's build took when it was last given back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Built {
    /// Bytes on disk of everything git ignores in it; `0` for none.
    pub(super) bytes: u64,
    /// When the checkout was given back with that build.
    pub(super) at: DateTime<Utc>,
}

/// What the pool at `home` last measured of checkout `number`'s build.
pub(super) async fn built(home: &Path, number: u32) -> Option<Built> {
    let bytes = tokio::fs::read(built_path(home, number)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Records what `roots` — what git ignores in checkout `number` of the
/// pool at `home`, given back at `at` — take on disk: at once when there
/// are none, and otherwise walked off the caller's way and recorded
/// unless a later give-back already was.
pub(super) async fn measure(home: &Path, number: u32, roots: Vec<PathBuf>, at: DateTime<Utc>) {
    let path = built_path(home, number);
    if roots.is_empty() {
        record(&path, Built { bytes: 0, at }).await;
        return;
    }
    tokio::spawn(async move {
        let Ok(bytes) = tokio::task::spawn_blocking(move || super::walk::on_disk(&roots)).await
        else {
            return;
        };
        let newer = built_at(&path).await.is_some_and(|recorded| recorded > at);
        if !newer {
            record(&path, Built { bytes, at }).await;
        }
    });
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

async fn built_at(path: &Path) -> Option<DateTime<Utc>> {
    let bytes = tokio::fs::read(path).await.ok()?;
    serde_json::from_slice::<Built>(&bytes)
        .ok()
        .map(|built| built.at)
}

/// Writes `built` at `path` whole: through a file beside it, renamed into
/// place, so a reader never meets half of it.
async fn record(path: &Path, built: Built) {
    let Ok(json) = serde_json::to_vec(&built) else {
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
