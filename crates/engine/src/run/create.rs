//! Run creation: freezing a run's anatomy on disk and its `run_created`
//! birth event, before anything executes it.

use std::path::{Path, PathBuf};

use yunta_core::durable::{self, Durability};
use yunta_core::events::{ArtifactId, EventPayload, RecordedOrigin, RunCreatedPayload};
use yunta_core::{
    ContentHash, InputName, Manifest, ModeName, NodeId, Redactor, RunId, SecretSource,
};
use yunta_storage::AsyncStorage;

use crate::run_log::RunLog;

use super::RunError;
use yunta_core::events::RunEvent;

mod documents;

use self::documents::{birth_registrations, hold_inherited_findings, register_birth_documents};

/// How a run comes by an artifact before any of its nodes runs: a
/// document one of its `inputs:` named, or what another run — a
/// predecessor, a parent, a sibling — hands over. Nothing else exists at
/// birth, so nothing else is representable here.
///
/// Narrower than [`yunta_core::events::RecordedOrigin`], which every acceptance of a run's
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

impl From<&BirthOrigin> for RecordedOrigin {
    fn from(origin: &BirthOrigin) -> Self {
        match origin {
            BirthOrigin::Input { input } => RecordedOrigin::Input {
                input: input.clone(),
            },
            BirthOrigin::Inherited { run, producer } => RecordedOrigin::Inherited {
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
    /// The measurement of the lineage this run is born into: the root's,
    /// named by the run that took it. `None` for a run that starts a
    /// lineage — it measures on its first wake, if its config names a
    /// suite.
    pub baseline: Option<&'a super::BirthBaseline>,
    /// What the run's commands will run with, recorded on `run_created`
    /// so a later wake can tell whether that changed.
    pub environment: Option<yunta_core::events::ExecutionEnvironment>,
    /// What the run's log redacts with, resolved by the caller through
    /// [`run_redactor`] before it takes anything a refused run would leave.
    pub redactor: &'a Redactor,
    /// The bytes of the frozen inputs `manifest` names by hash. They are
    /// written as objects, and the frozen skills and executors materialized
    /// beside them, before `manifest.yaml` exists.
    pub blobs: &'a [(ContentHash, Vec<u8>)],
}

/// The redactor a run of `manifest` works with: every secret its config
/// names, as `secrets` binds them. A caller resolves it before it takes a
/// checkout, so a run that a short secret refuses takes nothing.
pub fn run_redactor(manifest: &Manifest, secrets: &dyn SecretSource) -> Result<Redactor, RunError> {
    Ok(Redactor::of(&manifest.config.secret_names(), secrets)?)
}

/// Creates the run's anatomy: run.dir with `artifacts/` and
/// `scratch/`, the frozen `manifest.yaml`, the `run_created` event,
/// then one acceptance per birth artifact — with the tasks of every
/// tasks document among them registered beside it, `done` the ones
/// another run finished at a commit `worktree` already carries — and
/// last the run's baseline, captured on `worktree`. Returns the run
/// directory.
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
/// log. `"default"` — the caller's choice when nothing else applies, and
/// the name a log with no mode recorded reads back as — always passes: a
/// workflow declaring no `modes:` at all has nothing to validate a name
/// against, so every node stays schedulable. A workflow that *does*
/// declare `modes:` rejects any other unrecognized name.
pub async fn create_run(
    params: CreateRunParams<'_>,
    storage: &AsyncStorage,
    supervision: crate::process::Supervision<'_>,
) -> Result<PathBuf, RunError> {
    // One clock per birth: whatever the caller tells the time by is what
    // stamps the log and what judges a lock's holder.
    let clock = supervision.clock;
    let CreateRunParams {
        run_id,
        manifest,
        runs_root,
        mode,
        worktree,
        promoted_from,
        artifacts,
        baseline,
        environment,
        redactor,
        blobs,
    } = params;
    check_mode(manifest, mode)?;

    // Everything the run must be able to answer for is resolved before
    // it exists: a source whose log cannot be read leaves no run
    // directory and no `run_created` behind.
    let documents = birth_registrations(artifacts, worktree, storage, supervision).await?;
    let opens_on_base = manifest.isolation == yunta_core::Isolation::Worktree
        && holds_its_base(worktree, &manifest.base_commit, supervision).await?;

    let run_dir = runs_root.join(run_id.as_str());
    make_run_dir(runs_root, &run_dir).await?;

    // The frozen inputs are on disk before the manifest names them: a run
    // whose `manifest.yaml` exists can read everything it consumes.
    crate::frozen::store::write_at_birth(&run_dir, blobs, manifest.frozen.as_ref()).await?;
    write_manifest(&run_dir, manifest)?;

    let log = RunLog::new(storage, run_id, clock, redactor);
    let created = run_created(
        manifest,
        mode,
        promoted_from,
        worktree,
        environment,
        opens_on_base,
    );
    log.record(None, created).await?;

    register_birth_documents(&log, &run_dir, artifacts, &documents, worktree, supervision).await?;
    if promoted_from.is_some() {
        hold_inherited_findings(&log, artifacts).await?;
    }

    // Last: the measurement of the lineage this run is born into, if
    // it was born into one. What its own tree did is not a birth fact —
    // measuring is something an invocation does, and no invocation has
    // woken this run.
    if let Some(baseline) = baseline {
        record_baseline(&log, baseline).await?;
    }
    Ok(run_dir)
}

/// Refuses a mode the workflow does not declare. The `default` name always
/// passes, and any other name passes only where the workflow declares it.
fn check_mode(manifest: &Manifest, mode: &ModeName) -> Result<(), RunError> {
    if *mode == ModeName::default() {
        return Ok(());
    }
    match &manifest.workflow.modes {
        Some(modes) if !modes.contains_key(mode) => Err(RunError::UnknownMode {
            workflow: manifest.workflow.name.clone(),
            mode: mode.clone(),
            declared: modes
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        }),
        Some(_) => Ok(()),
        None => Err(RunError::UnknownMode {
            workflow: manifest.workflow.name.clone(),
            mode: mode.clone(),
            declared: "(none — this workflow declares no modes:)".to_string(),
        }),
    }
}

/// Creates the runs root, then the run directory with its `artifacts/` and
/// `scratch/` directories inside it. The run directory must not already
/// exist: a run is born once.
async fn make_run_dir(runs_root: &Path, run_dir: &Path) -> Result<(), RunError> {
    tokio::fs::create_dir_all(runs_root)
        .await
        .map_err(|source| RunError::Io {
            context: format!("create runs root `{}`", runs_root.display()),
            source,
        })?;
    match tokio::fs::create_dir(run_dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RunError::RunDirExists {
                path: run_dir.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(RunError::Io {
                context: format!("create run directory `{}`", run_dir.display()),
                source,
            });
        }
    }
    for dir in [
        crate::run_dir::artifacts_view(run_dir),
        run_dir.join(crate::run_dir::SCRATCH_DIR),
    ] {
        tokio::fs::create_dir(&dir)
            .await
            .map_err(|source| RunError::Io {
                context: format!("create run directory `{}`", dir.display()),
                source,
            })?;
    }
    Ok(())
}

/// Writes the frozen `manifest.yaml`, which the caller writes only after the
/// inputs it names are on disk.
fn write_manifest(run_dir: &Path, manifest: &Manifest) -> Result<(), RunError> {
    let manifest_path = crate::run_dir::manifest_path(run_dir);
    let bytes = yunta_core::persisted::PersistedDoc::of(manifest.clone())
        .write()
        .map_err(|e| RunError::ManifestWrite {
            path: manifest_path.clone(),
            detail: e.to_string(),
        })?;
    // blocking: one small file, replaced whole on this thread.
    durable::replace(&manifest_path, &bytes, Durability::Atomic).map_err(|source| RunError::Io {
        context: format!("write `{}`", manifest_path.display()),
        source,
    })
}

/// The `run_created` event: what the run is, as its birth freezes it.
fn run_created(
    manifest: &Manifest,
    mode: &ModeName,
    promoted_from: Option<&RunId>,
    worktree: &Path,
    environment: Option<yunta_core::events::ExecutionEnvironment>,
    opens_on_base: bool,
) -> EventPayload {
    EventPayload::Run(RunEvent::Created(RunCreatedPayload {
        manifest_hash: manifest.manifest_hash(),
        // Every reader of the run's tree reads it here, rather than
        // composing a path of its own.
        checkout: (manifest.isolation == yunta_core::Isolation::Worktree)
            .then(|| worktree.to_path_buf()),
        // Every declared input as the manifest froze it — provided
        // or defaulted, already validated: what the run used.
        inputs: manifest
            .inputs
            .iter()
            .map(|(name, value)| (name.to_string(), serde_json::Value::String(value.clone())))
            .collect(),
        mode: mode.clone(),
        promoted_from: promoted_from.cloned(),
        // Resolved once here: the declared range as written, or the binary's
        // own schema when none is declared.
        yunta_schema: Some(
            manifest
                .workflow
                .yunta_schema
                .clone()
                .unwrap_or_else(|| yunta_core::SchemaRange::exactly(yunta_core::YUNTA_SCHEMA)),
        ),
        base_branch: manifest.base_branch.clone(),
        base_commit: manifest.base_commit.clone(),
        environment: environment.map(Box::new),
        // Decided here, from the config the run froze, and read back
        // off the log by every wake: a run's graph never depends on
        // a config read after its birth.
        left_out: yunta_core::left_out(&manifest.workflow, &manifest.config),
        opens_on_base,
    }))
}

/// Records the measurement of the lineage the run is born into: the root's
/// capture first, then each hold that goes with it.
async fn record_baseline(
    log: &RunLog<'_>,
    baseline: &super::BirthBaseline,
) -> Result<(), RunError> {
    log.record(
        None,
        EventPayload::Run(RunEvent::BaselineCaptured(baseline.captured())),
    )
    .await?;
    for held in baseline.held() {
        log.record(None, EventPayload::Run(RunEvent::BaselineHeld(held)))
            .await?;
    }
    Ok(())
}

/// Whether `worktree` holds exactly what `base` does: nothing a person
/// left uncommitted in it, nothing a birth brought with it. A tree that
/// cannot be read answers no, and so does a base the repository does not
/// have, since its tree cannot be read either; the run measures in it. A
/// cancelled read is the run's cancellation, not an answer.
async fn holds_its_base(
    worktree: &std::path::Path,
    base: &yunta_core::CommitSha,
    supervision: crate::process::Supervision<'_>,
) -> Result<bool, RunError> {
    let nothing_covered = yunta_core::protected::Pathspecs::default();
    let holds = match crate::task_cycle::content_of(worktree, &nothing_covered, supervision).await {
        Ok(holds) => holds,
        Err(error) => match RunError::from(error) {
            RunError::Cancelled => return Err(RunError::Cancelled),
            _ => return Ok(false),
        },
    };
    let base_tree = format!("{base}^{{tree}}");
    match crate::git::read(worktree, &["rev-parse", &base_tree], supervision).await {
        Ok(committed) => Ok(holds.as_str() == committed.trim()),
        Err(error) if error.cancelled() => Err(error.into()),
        Err(_) => Ok(false),
    }
}
