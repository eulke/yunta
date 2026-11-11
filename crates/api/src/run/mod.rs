//! See [`crate`]. A run of a project, opened: where it lives, the manifest it
//! froze and its whole log, found by its id or by what a person typed for it.
//!
//! Opening is the one prologue every operation on a run shares, so a run
//! nobody has is refused in one sentence, naming where it was looked for and
//! the run it was one slip away from, whichever interface asked.

mod follow;
mod page;
mod reading;
mod receipt;
mod reference;
mod stats;
mod verification;

use std::path::{Path, PathBuf};

use yunta_core::events::StoredEvent;
use yunta_core::persisted::PersistedDoc;
use yunta_core::{Manifest, RunId};
use yunta_render::refusal::Refused;
use yunta_storage::Storage;

use crate::{Error, Failure, Project};

pub use follow::{FollowedRun, Following, RunNews, RunRegistry};
pub use reading::RunReading;
pub use reference::RunRef;
pub use stats::StatsReading;

/// A run of a project, opened: where it lives, the manifest it froze, and
/// its whole log, asked again through the project it was opened in.
///
/// Not render's run page, which a person reads drawn from it.
pub struct OpenedRun<'p> {
    /// The project it was opened in, whose clock, store and tree it reads.
    pub(crate) project: &'p Project,
    pub id: RunId,
    /// The directory the run keeps its manifest, its lock and its documents
    /// in.
    pub dir: PathBuf,
    pub manifest: PersistedDoc<Manifest>,
    pub events: Vec<StoredEvent>,
}

impl Project {
    /// The run `id`, opened: refused, naming where it was looked for and the
    /// run it is one slip away from, when this machine has no record of it.
    pub async fn run(&self, id: &RunId) -> Result<OpenedRun<'_>, Error> {
        let storage = self.storage().await?;
        let events = storage
            .events_for_run(id.clone())
            .await
            .map_err(Failure::new)?;
        let dir = self.run_dir(id.as_str());
        if events.is_empty() && dir.is_none() {
            return Err(self.unknown_run(id).into());
        }
        let dir = dir.unwrap_or_else(|| self.runs_root.join(id.as_str()));
        let manifest = read_manifest(&yunta_engine::run_dir::manifest_path(&dir))?;
        Ok(OpenedRun {
            project: self,
            id: id.clone(),
            dir,
            manifest,
            events,
        })
    }

    /// The refusal of a run this machine has no record of: where it was
    /// looked for, as a person reads a path, and the run it is one slip away
    /// from.
    fn unknown_run(&self, id: &RunId) -> Refused {
        let workspace = &self.workspace;
        Refused::UnknownRun {
            run: id.clone(),
            roots: self
                .run_roots()
                .iter()
                .map(|root| {
                    surco::paths::shown(root, &workspace.dir, workspace.env.home.as_deref())
                })
                .collect(),
            near: self.near_run(id),
        }
    }

    /// The run `id` is one slip away from, by its handle or by its whole
    /// id, when exactly one is.
    fn near_run(&self, id: &RunId) -> Option<String> {
        let runs = Storage::open(&self.storage_path).ok()?.list_runs().ok()?;
        let typed = id.as_str().to_ascii_uppercase();
        let named: Vec<String> = runs
            .iter()
            .flat_map(|run| [run.run_id.handle().to_string(), run.run_id.to_string()])
            .collect();
        surco::text::nearest(&typed, named.iter().map(String::as_str)).map(str::to_string)
    }

    /// Where a run is looked for, in search order — what a refusal names
    /// so a person knows where this binary did look.
    fn run_roots(&self) -> Vec<PathBuf> {
        let here = self.runs_root.clone();
        let default = self.user_root.join("runs");
        std::iter::once(here.clone())
            .chain(Some(default).filter(|default| *default != here))
            .collect()
    }
}

/// A run's manifest that could not be read off the disk.
#[derive(Debug, thiserror::Error)]
#[error("failed to read run manifest at {}: {source}", path.display())]
struct ManifestUnread {
    path: PathBuf,
    source: std::io::Error,
}

/// The manifest at `path`, as the run froze it.
pub(crate) fn read_manifest(path: &Path) -> Result<PersistedDoc<Manifest>, Failure> {
    let bytes = std::fs::read(path).map_err(|source| {
        Failure::new(ManifestUnread {
            path: path.to_path_buf(),
            source,
        })
    })?;
    Manifest::read_frozen(&bytes).map_err(Failure::new)
}
