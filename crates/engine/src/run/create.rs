//! Run creation: freezing a run's anatomy on disk and its `run_created`
//! birth event, before anything executes it.

use std::path::{Path, PathBuf};

use yunta_core::events::{ArtifactId, ArtifactOrigin, EventPayload, RunCreatedPayload};
use yunta_core::{Clock, Manifest, ModeName, RunId, ARTIFACTS_DIR};
use yunta_storage::AsyncStorage;

use crate::artifacts::accept;
use crate::run_log::RunLog;

use super::RunError;

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
    /// Which run it comes from, and who produced it there.
    pub origin: ArtifactOrigin,
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
    /// The predecessor this run inherits from, if any.
    pub promoted_from: Option<&'a RunId>,
    /// The artifacts the run holds from birth, accepted right after
    /// `run_created` — a run that exists in the log names every one of
    /// them.
    pub artifacts: &'a [BirthArtifact],
}

/// Creates the run's anatomy: run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, the `run_created` event, and
/// then one acceptance per birth artifact — with the tasks of a document
/// the run was given registered beside it. Returns the run directory.
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

    let log = RunLog::new(storage, run_id, clock);
    log.record(
        None,
        EventPayload::RunCreated(RunCreatedPayload {
            manifest_hash: manifest.manifest_hash(),
            // Every declared input as the manifest froze it — provided
            // or defaulted, already validated: what the run used.
            inputs: manifest
                .inputs
                .iter()
                .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
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
                    .unwrap_or_else(|| format!("={}", yunta_core::YUNTA_SCHEMA)),
            ),
            base_branch: manifest.base_branch.clone(),
            base_commit: manifest.base_commit.clone(),
        }),
    )
    .await?;

    for artifact in artifacts {
        accept(
            &log,
            &run_dir,
            None,
            artifact.artifact.clone(),
            &artifact.bytes,
            artifact.origin.clone(),
        )
        .await?;
        register_input_tasks(&log, artifact).await?;
    }

    Ok(run_dir)
}

/// Registers every task of a tasks document the run was *given*.
///
/// Accepting a document says what the run holds; a `task_registered`
/// says what the run has to do about it, and until one exists a task is
/// not a task of this run. A node's close states both for what that node
/// produced — and a document that came in as an input has no node that
/// will ever produce it, so its birth is the only place the second fact
/// can be stated. What a run inherits from another run is left alone:
/// there the tasks belong to a chain whose own nodes register them where
/// they produce them.
async fn register_input_tasks(log: &RunLog<'_>, artifact: &BirthArtifact) -> Result<(), RunError> {
    if !matches!(artifact.origin, ArtifactOrigin::Input { .. })
        || artifact.artifact
            != (ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Tasks,
            })
    {
        return Ok(());
    }
    // The bytes are what the run accepted, so they read back as the
    // document they were rendered from; a reading that fails anyway
    // describes a run whose own birth artifact is not what it says it
    // is, and says so rather than registering half a document.
    let tasks: yunta_core::TasksFile =
        yunta_core::shape::read(&artifact.bytes, artifact.artifact.view_name()).map_err(
            |report| RunError::Broken {
                diagnostic: report.to_string(),
            },
        )?;
    for task in &tasks.tasks {
        log.record(
            None,
            EventPayload::TaskRegistered(yunta_core::events::TaskRegisteredPayload {
                task_id: task.id.clone(),
                criteria: task.criteria.iter().map(Into::into).collect(),
                scope: task.scope.clone(),
                depends_on: task.depends_on.clone(),
            }),
        )
        .await?;
    }
    Ok(())
}
