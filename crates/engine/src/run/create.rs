//! Run creation: freezing a run's anatomy on disk and its `run_created`
//! birth event, before anything executes it.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use yunta_core::events::{ArtifactId, ArtifactOrigin, EventPayload, RunCreatedPayload};
use yunta_core::{
    Clock, CommitSha, InputName, Manifest, ModeName, NodeId, RunId, TaskId, ARTIFACTS_DIR,
};
use yunta_storage::AsyncStorage;

use crate::artifacts::accept;
use crate::run_log::RunLog;
use crate::tasks::Provenance;

use super::RunError;
use yunta_core::events::RunEvent;

/// How a run comes by an artifact before any of its nodes runs: a
/// document one of its `inputs:` named, or what another run — a
/// predecessor, a parent, a sibling — hands over. Nothing else exists at
/// birth, so nothing else is representable here.
///
/// Narrower than [`ArtifactOrigin`], which every acceptance of a run's
/// whole life shares: what a run is born holding it did not produce,
/// derive or receive an answer to, and a birth that names one of those
/// is a state nobody can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BirthOrigin {
    /// A `type: document` input, named by the input it came in as.
    Input { input: InputName },
    /// Another run's artifact. `producer` is the node that produced it
    /// there, absent when that run acquired it without a node either.
    Inherited {
        run: RunId,
        producer: Option<NodeId>,
    },
}

impl From<&BirthOrigin> for ArtifactOrigin {
    fn from(origin: &BirthOrigin) -> Self {
        match origin {
            BirthOrigin::Input { input } => ArtifactOrigin::Input {
                input: input.clone(),
            },
            BirthOrigin::Inherited { run, producer } => ArtifactOrigin::Inherited {
                run: run.clone(),
                producer: producer.clone(),
            },
        }
    }
}

/// An artifact a run carries from birth: a document its `inputs:`
/// named, what a parent mounts into a child, or what a successor
/// inherits from its predecessor.
///
/// It carries its own identity and origin because the run that receives
/// it cannot derive either: only where the bytes came from says what
/// artifact they are and how the run came by them. A mount that renames
/// an opaque artifact hands over the renamed identity, which is what the
/// receiving run holds it as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BirthArtifact {
    /// What the artifact is in the run receiving it.
    pub artifact: ArtifactId,
    /// How the run came by it, and from whom.
    pub origin: BirthOrigin,
    pub bytes: Vec<u8>,
}

/// What [`create_run`] freezes: the run's identity and its
/// declared birth facts, bundled — `storage`/`clock` stay separate
/// arguments because they are the caller's *infrastructure*, not this
/// run's data.
pub struct CreateRunParams<'a> {
    pub run_id: &'a RunId,
    pub manifest: &'a Manifest,
    pub runs_root: &'a Path,
    /// Frozen into `run_created.mode` and never re-resolved.
    pub mode: &'a ModeName,
    /// The working tree this run's nodes execute in, already prepared:
    /// a birth has to be able to ask that tree what it already carries
    /// before the run says what it has to do.
    pub worktree: &'a Path,
    /// The predecessor this run inherits from, if any.
    pub promoted_from: Option<&'a RunId>,
    /// The artifacts the run holds from birth, accepted right after
    /// `run_created` — a run that exists in the log names every one of
    /// them.
    pub artifacts: &'a [BirthArtifact],
}

/// Creates the run's anatomy: run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, the `run_created` event, and
/// then one acceptance per birth artifact — with the tasks of every
/// tasks document among them registered beside it, `done` the ones
/// another run finished at a commit `worktree` already carries. Returns
/// the run directory.
///
/// `run_created` comes first because it is the run: replay reads it
/// before anything else, so an artifact a run is born holding is a fact
/// stated about a run that already exists.
///
/// The run directory must not exist: a run is born once, and an id is
/// never reused ([`RunError::RunDirExists`]).
///
/// `mode` is frozen into `run_created.mode` right here and
/// never re-resolved again — a resume reads the same name back off the
/// log. `"default"` — the caller's choice when nothing else applies,
/// same sentinel `events::run_mode` falls back to for a log with no
/// mode recorded — always passes: a workflow declaring no `modes:` at
/// all has nothing to validate a name against, and every node stays
/// schedulable, exactly the behavior before modes existed. A workflow
/// that *does* declare `modes:` rejects any other unrecognized name.
pub async fn create_run(
    params: CreateRunParams<'_>,
    storage: &AsyncStorage,
    clock: &dyn Clock,
) -> Result<PathBuf, RunError> {
    let CreateRunParams {
        run_id,
        manifest,
        runs_root,
        mode,
        worktree,
        promoted_from,
        artifacts,
    } = params;
    if *mode != ModeName::default() {
        match &manifest.workflow.modes {
            Some(modes) if !modes.contains_key(mode) => {
                return Err(RunError::UnknownMode {
                    workflow: manifest.workflow.name.clone(),
                    mode: mode.clone(),
                    declared: modes
                        .keys()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
            Some(_) => {}
            None => {
                return Err(RunError::UnknownMode {
                    workflow: manifest.workflow.name.clone(),
                    mode: mode.clone(),
                    declared: "(none — this workflow declares no modes:)".to_string(),
                });
            }
        }
    }

    // Everything the run must be able to answer for is resolved before
    // it exists: a source whose log cannot be read leaves no run
    // directory and no `run_created` behind.
    let documents = birth_registrations(artifacts, worktree, storage).await?;

    let run_dir = runs_root.join(run_id.as_str());
    tokio::fs::create_dir_all(runs_root)
        .await
        .map_err(|source| RunError::Io {
            context: format!("create runs root `{}`", runs_root.display()),
            source,
        })?;
    match tokio::fs::create_dir(&run_dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RunError::RunDirExists { path: run_dir });
        }
        Err(source) => {
            return Err(RunError::Io {
                context: format!("create run directory `{}`", run_dir.display()),
                source,
            });
        }
    }
    for dir in [
        run_dir.join(ARTIFACTS_DIR),
        run_dir.join(crate::run_dir::SCRATCH_DIR),
    ] {
        tokio::fs::create_dir(&dir)
            .await
            .map_err(|source| RunError::Io {
                context: format!("create run directory `{}`", dir.display()),
                source,
            })?;
    }

    let manifest_path = run_dir.join("manifest.yaml");
    let yaml = yunta_core::yaml::to_string(manifest).map_err(|e| RunError::ManifestWrite {
        path: manifest_path.clone(),
        detail: e.to_string(),
    })?;
    tokio::fs::write(&manifest_path, yaml)
        .await
        .map_err(|source| RunError::Io {
            context: format!("write `{}`", manifest_path.display()),
            source,
        })?;

    // A birth is written before a run exists to declare secrets
    // against: `run_created` carries the manifest's hash and the
    // inputs the caller resolved, never a session's words.
    let nothing_to_redact = yunta_core::Redactor::default();
    let log = RunLog::new(storage, run_id, clock, &nothing_to_redact);
    log.record(
        None,
        EventPayload::Run(RunEvent::Created(RunCreatedPayload {
            manifest_hash: manifest.manifest_hash(),
            // Every declared input as the manifest froze it — provided
            // or defaulted, already validated: what the run used.
            inputs: manifest
                .inputs
                .iter()
                .map(|(name, value)| (name.to_string(), serde_json::Value::String(value.clone())))
                .collect(),
            mode: mode.clone(),
            promoted_from: promoted_from.cloned(),
            // Resolved once here — declared range as
            // written, or the binary's own schema when absent (the
            // reference text's "inferred from the binary").
            yunta_schema: Some(
                manifest
                    .workflow
                    .yunta_schema
                    .clone()
                    .unwrap_or_else(|| yunta_core::SchemaRange::exactly(yunta_core::YUNTA_SCHEMA)),
            ),
            base_branch: manifest.base_branch.clone(),
            base_commit: manifest.base_commit.clone(),
        })),
    )
    .await?;

    register_birth_documents(&log, &run_dir, artifacts, &documents).await?;

    Ok(run_dir)
}

/// One birth artifact that is a tasks document: what it says, and what
/// of it the run's own tree already has — `None` when an input named it,
/// because nobody did anything about those tasks before.
struct BirthDocument {
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
async fn birth_registrations(
    artifacts: &[BirthArtifact],
    worktree: &Path,
    storage: &AsyncStorage,
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
                        let events = storage.events_for_run(run.clone()).await?;
                        slot.insert(crate::tasks::standing_of(run, &events)?)
                    }
                };
                Some(
                    crate::tasks::carried_into(
                        standing,
                        &document,
                        worktree,
                        // Creating a run has no run to cancel yet: the
                        // git that answers what the tree carries is the
                        // caller's, bounded by its own invocation.
                        crate::process::Supervision::none(),
                    )
                    .await?,
                )
            }
        };
        documents.push(Some(BirthDocument { document, carried }));
    }
    Ok(documents)
}

/// Accepts every birth artifact in order and, for each tasks document,
/// registers what the run has to do about it.
///
/// Interleaved rather than accepted in one pass and registered in
/// another, so a second tasks document is planned against a log that
/// already holds the first one's registrations.
async fn register_birth_documents(
    log: &RunLog<'_>,
    run_dir: &Path,
    artifacts: &[BirthArtifact],
    documents: &[Option<BirthDocument>],
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
        }
    }
    Ok(())
}
