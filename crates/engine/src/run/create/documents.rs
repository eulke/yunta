//! The documents a birth artifact brings with it: the tasks document a run is
//! born holding, which registers the tasks it has to do, and the findings a
//! promotion successor holds as its own.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::Path;

use yunta_core::events::{ArtifactId, EventPayload};
use yunta_core::{CommitSha, RunId, TaskId};
use yunta_storage::AsyncStorage;

use crate::artifacts::accept;
use crate::replay::RunView;
use crate::run::RunError;
use crate::run_log::RunLog;
use crate::tasks::Provenance;

use super::{BirthArtifact, BirthOrigin};

/// One birth artifact that is a tasks document: what it says, and what
/// of it the run's own tree already has — `None` when an input named it,
/// because nobody did anything about those tasks before.
pub(super) struct BirthDocument {
    document: yunta_core::TasksFile,
    carried: Option<BTreeMap<TaskId, CommitSha>>,
}

impl BirthDocument {
    fn provenance(&self) -> Provenance<'_> {
        match &self.carried {
            Some(carried) => Provenance::Inherited { carried },
            None => Provenance::Fresh,
        }
    }
}

/// Reads every tasks document among `artifacts` and, for one another run
/// hands over, asks `worktree` which of the tasks that run finished it
/// already carries — one log read per source run. Entries line up with
/// `artifacts` by position, `None` for an artifact that is not a tasks
/// document.
///
/// Before the run has a directory or a `run_created`, for the same
/// reason an invalid `type: document` input refuses the birth: a run
/// that cannot answer for what it is born holding is one nobody can
/// resume, and there is nothing to clean up if it never exists.
pub(super) async fn birth_registrations(
    artifacts: &[BirthArtifact],
    worktree: &Path,
    storage: &AsyncStorage,
    supervision: crate::process::Supervision<'_>,
) -> Result<Vec<Option<BirthDocument>>, RunError> {
    let tasks = ArtifactId::Interpreted {
        kind: yunta_core::ArtifactKind::Tasks,
    };
    let mut standings: BTreeMap<RunId, crate::tasks::Standing> = BTreeMap::new();
    let mut documents = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        if artifact.artifact != tasks {
            documents.push(None);
            continue;
        }
        // The bytes are what the run accepts, so they read back as the
        // document they were rendered from; a reading that fails anyway
        // describes a birth artifact that is not what it says it is, and
        // says so rather than registering half a document.
        let document = yunta_core::shape::read(&artifact.bytes, artifact.artifact.view_name())
            .map_err(|report| RunError::Broken {
                diagnostic: report.to_string(),
            })?;
        let carried = match &artifact.origin {
            BirthOrigin::Input { .. } => None,
            BirthOrigin::Inherited { run, .. } => {
                let standing = match standings.entry(run.clone()) {
                    Entry::Occupied(held) => held.into_mut(),
                    Entry::Vacant(slot) => {
                        let view = RunView::of(storage.events_for_run(run.clone()).await?);
                        slot.insert(crate::tasks::standing_of(run, &view)?)
                    }
                };
                Some(crate::tasks::carried_into(standing, &document, worktree, supervision).await?)
            }
        };
        documents.push(Some(BirthDocument { document, carried }));
    }
    Ok(documents)
}

/// A promotion successor holds the findings its predecessor left
/// unsettled as the run's own: the run-level findings document it
/// inherits, entry by entry, so its gates show them and a person can
/// settle them — not only a context source that reads them.
pub(super) async fn hold_inherited_findings(
    log: &RunLog<'_>,
    artifacts: &[BirthArtifact],
) -> Result<(), RunError> {
    let findings = ArtifactId::Interpreted {
        kind: yunta_core::ArtifactKind::Findings,
    };
    let run_level = artifacts.iter().filter(|artifact| {
        artifact.artifact == findings
            && matches!(
                artifact.origin,
                BirthOrigin::Inherited { producer: None, .. }
            )
    });
    for artifact in run_level {
        let held = yunta_core::shape::read::<yunta_core::HeldFindings>(
            &artifact.bytes,
            "the inherited findings",
        )
        .map_err(crate::artifacts::HeldError::from)?;
        for entry in held.findings {
            let finding = yunta_core::events::Finding::from(entry);
            log.record(
                None,
                EventPayload::Findings(yunta_core::events::FindingEvent::Posted(
                    yunta_core::events::FindingPostedPayload { finding },
                )),
            )
            .await?;
        }
    }
    Ok(())
}

/// Accepts every birth artifact in order and, for each tasks document,
/// registers what the run has to do about it and what its tasks may reach
/// in `tree`.
///
/// Interleaved rather than accepted in one pass and registered in
/// another, so a second tasks document is planned against a log that
/// already holds the first one's registrations.
pub(super) async fn register_birth_documents(
    log: &RunLog<'_>,
    run_dir: &Path,
    artifacts: &[BirthArtifact],
    documents: &[Option<BirthDocument>],
    tree: &Path,
    supervision: crate::process::Supervision<'_>,
) -> Result<(), RunError> {
    for (artifact, birth) in artifacts.iter().zip(documents) {
        accept(
            log,
            run_dir,
            None,
            artifact.artifact.clone(),
            &artifact.bytes,
            (&artifact.origin).into(),
        )
        .await?;
        if let Some(birth) = birth {
            crate::tasks::register(log, None, &birth.document, birth.provenance()).await?;
            crate::tasks::derive_reach(log, None, &birth.document, tree, supervision).await?;
        }
    }
    Ok(())
}
