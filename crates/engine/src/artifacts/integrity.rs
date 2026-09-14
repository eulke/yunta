//! Whether a run still holds the bytes its own log says it accepted.
//!
//! An acceptance names an object, and the object answers for itself: its
//! name is what its content hashes to. Reading every one of them is
//! therefore the whole of the question "are this run's artifacts still
//! its artifacts", and it is asked where the answer changes what happens
//! next — when a resume wakes the run, and when `yunta verify` is asked
//! about it. The `artifacts/` directory is not part of the question: it
//! is the view, regenerated from the store, and deleting it says nothing
//! about what the run holds.
//!
//! **A log older than the object store is accounted for, not failed.**
//! Every artifact such a log states is an `artifact_written`, which the
//! fold carries with [`ArtifactOrigin::Legacy`]: the run recorded a hash
//! but never wrote an object, so there is nothing under `objects/` for
//! that hash and there never was. Checking it against the store would
//! make every run written before the store irresumable — a verification
//! its format cannot satisfy — and checking it against the file its log
//! named would give `artifacts/` two meanings, authoritative for an old
//! run and derived for a new one, with the run's age deciding which. So
//! it is counted as what it is: an artifact this binary cannot check,
//! reported with its count wherever the verification is reported. Same
//! reading as an event kind from the future — derive what can be
//! derived, say what could not be, never `broken` for it.

use std::path::Path;

use yunta_core::events::{ArtifactOrigin, StoredEvent};
use yunta_core::RunId;

use super::{describe, ObjectError, RunArtifacts};

/// One artifact whose bytes are not the bytes the run accepted.
#[derive(Debug)]
pub struct ArtifactFault {
    /// The artifact, named as every diagnostic about one names it: the
    /// path of its view, which is the file a reader opens.
    pub artifact: String,
    /// What the store answered — absent bytes, or bytes that are no
    /// longer what their own name says, with both hashes.
    pub error: ObjectError,
}

/// What a run's log says it holds, checked against the bytes it has.
#[derive(Debug, Default)]
pub struct ArtifactIntegrity {
    /// Artifacts whose object was read back and hashed to its own name.
    pub verified: usize,
    /// Artifacts named by a log written before the object store, which
    /// hold no object to check — see this module's own note.
    pub unverifiable: usize,
    /// Artifacts the run can no longer hand over as it accepted them.
    pub faults: Vec<ArtifactFault>,
}

impl ArtifactIntegrity {
    /// Reads every object the log of the run rooted at `run_dir` names.
    ///
    /// Total: an artifact ends up in exactly one of the three counts, so
    /// the result accounts for every acceptance the log carries.
    pub async fn of(run_dir: &Path, events: &[StoredEvent]) -> Self {
        let artifacts = RunArtifacts::of(run_dir, events);
        let mut integrity = ArtifactIntegrity::default();
        for held in artifacts.ledger().every() {
            if held.origin == ArtifactOrigin::Legacy {
                integrity.unverifiable += 1;
                continue;
            }
            match artifacts.bytes(held).await {
                Ok(_) => integrity.verified += 1,
                Err(error) => integrity.faults.push(ArtifactFault {
                    artifact: describe(held),
                    error,
                }),
            }
        }
        integrity
    }

    /// Why run `run_id` is broken, or `None` when every artifact it
    /// names is accounted for.
    ///
    /// The one sentence for a run whose store no longer answers for its
    /// log: it names the run, each artifact, and what the store said
    /// about it — which for content that changed is both hashes.
    pub fn diagnostic(&self, run_id: &RunId) -> Option<String> {
        if self.faults.is_empty() {
            return None;
        }
        let faults: Vec<String> = self
            .faults
            .iter()
            .map(|fault| format!("`{}`: {}", fault.artifact, fault.error))
            .collect();
        Some(format!(
            "run `{run_id}` no longer holds the bytes its log accepted for {} of the {} \
             artifact(s) it names: {}",
            self.faults.len(),
            self.verified + self.faults.len(),
            faults.join("; ")
        ))
    }

    /// What the verification could not check, as a sentence for a
    /// person — `None` when it checked everything the log names.
    pub fn unverifiable_detail(&self) -> Option<String> {
        (self.unverifiable > 0).then(|| {
            format!(
                "{} artifact(s) of this run are named by `artifact_written`, written before a run \
                 kept the bytes of its artifacts under `{}/`: no object answers for them and none \
                 ever did, so they are left unchecked and the {} artifact(s) the log accepted are \
                 the ones verified",
                self.unverifiable,
                super::store::OBJECTS_DIR,
                self.verified,
            )
        })
    }
}
