//! See [`super`]. The two mechanical guarantees a run makes about its own
//! evidence, checked apart.
//!
//! The event chain covers the log: every link recomputed from the bytes as
//! persisted, so an altered payload or a deleted, inserted or reordered event
//! surfaces with the seq it begins at. Integrity and order only — never
//! authenticity, which is a separate layer. The objects cover the artifacts:
//! every acceptance on that log names bytes under `objects/`, and each one is
//! read back and hashed against its own name — the same verification a resume
//! runs before it wakes the run.
//!
//! A run is verified by its id, never opened: opening reads the log as events,
//! and the chain answers about a log whose payloads no longer parse.

use yunta_core::receipt::EventChainStatus;
use yunta_core::RunId;
use yunta_engine::{ArtifactIntegrity, StoreError};
use yunta_render::page::{FaultCause, ObjectFault, ObjectsChecked, ObjectsRead, Verification};
use yunta_storage::{ChainVerification, Storage};

use crate::{Error, Failure, Project};

impl Project {
    /// What checking run `id`'s evidence finds: its chain first, which
    /// refuses a run the log does not know, then the objects its log names.
    pub async fn verification(&self, id: &RunId) -> Result<Verification, Error> {
        let storage = Storage::open(&self.storage_path).map_err(Failure::new)?;
        let chain = match storage.verify_chain(id).map_err(Failure::new)? {
            ChainVerification::Intact { events } => EventChainStatus::Intact { events },
            ChainVerification::Broken { seq, detail } => EventChainStatus::Broken { seq, detail },
        };
        Ok(Verification {
            run_id: id.clone(),
            chain,
            objects: self.objects_read(id, &storage).await,
        })
    }

    /// What reading back every object run `id`'s log names finds. Which
    /// objects to read comes from the log itself, so a log that no longer
    /// reads back as events leaves nothing to check against.
    async fn objects_read(&self, id: &RunId, storage: &Storage) -> ObjectsRead {
        let Some(run_dir) = self.run_dir(id.as_str()) else {
            // The log outlives the directory: `yunta gc` removes a run's
            // directory before purging its events.
            return ObjectsRead::NoDirectory {
                runs_root: self.runs_root.clone(),
            };
        };
        let events = match storage.events_for_run(id) {
            Ok(events) => events,
            Err(error) => return ObjectsRead::UnreadableLog(error.to_string()),
        };
        let integrity = ArtifactIntegrity::of(&run_dir, &events).await;
        ObjectsRead::Read(ObjectsChecked {
            verified: integrity.verified,
            unverifiable: integrity.unverifiable,
            faults: integrity
                .faults
                .into_iter()
                .map(|fault| ObjectFault {
                    artifact: fault.artifact,
                    cause: match fault.error {
                        StoreError::Object(problem) => FaultCause::Object(problem),
                        StoreError::Io { context, .. } => FaultCause::Failed { step: context },
                    },
                })
                .collect(),
        })
    }
}
