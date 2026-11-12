//! Per-run process registry: `run.dir/scratch/engine.json`,
//! the one thing that lets a *separate* process — `yunta cancel`, which
//! shares no memory with the run it stops — find and signal a live run's
//! process tree so cancellation can always tear the whole tree down, not
//! just the top process. Scratch, deliberately: it is ephemeral process
//! state, not an artifact and not event-log truth; it is written when
//! `execute_run` starts, updated as sessions/hooks/executors spawn and
//! close, and deleted at every terminal. A terminal keeps one thing: a
//! process group the engine could not stop, while that group still runs.
//! The file then names only those groups, so `yunta cancel` and `yunta
//! close` can signal them and the checkout pool keeps their checkout.
//!
//! While its engine lives, the file also names the socket that engine
//! serves the run's words on ([`crate::said`]), and the registry holds that
//! socket open: clearing the registry at a terminal closes it and takes its
//! file away, so a reader never finds a socket named for an engine that
//! stopped. A registry kept for a group names none.
//!
//! Registration is best-effort bookkeeping around processes that are
//! already owned and killed by the in-process paths: a failed write
//! here degrades to a `tracing` warning, never to a failed run — but it
//! degrades *loudly*, never silently.

use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use yunta_core::persisted::Persisted;
use yunta_core::process::group::group_runs;
use yunta_core::process::signal::Liveness;
use yunta_core::{Pid, RelativePath};
use yunta_replay::registry::{Engine, Prompt};

impl Persisted for EngineProcessFile {
    /// The version this binary writes; a newer one is refused, never guessed at.
    const SCHEMA_VERSION: u32 = 1;
    const NAME: &'static str = "process registry";
    // `engine.json` — the name is what a person opening it reads, and a
    // separate process parsing it at a crash should not need a YAML
    // reader to.
    const ENCODING: yunta_core::persisted::Encoding = yunta_core::persisted::Encoding::Json;
}

/// The file's whole content — small enough that every mutation rewrites
/// it atomically (tempfile + rename) rather than patching in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineProcessFile {
    /// Version of this file's own schema.
    #[serde(default)]
    pub schema_version: u32,
    /// The `yunta` process driving the run — signal it first (`yunta
    /// cancel` sends SIGINT here while it's alive, so the engine's own
    /// interrupt→kill path does the exterminating).
    pub engine_pid: Pid,
    /// When this engine started, as the host's process table reads it —
    /// what tells a live pid apart from a number the host handed to
    /// something else after a crash. Read there and not from the run's
    /// clock, because [`EngineProcessFile::liveness`] compares it
    /// against that same table — the reason the lock reads it there
    /// too.
    pub started_at: DateTime<Utc>,
    /// Process-group ids of live sessions/hooks/executors — what a
    /// post-crash `cancel` kills directly when `engine_pid` is gone.
    pub process_groups: Vec<Pid>,
    /// The question this engine holds at its terminal, while it asks one:
    /// a question asked live leaves nothing on the log until it is
    /// answered, and a reader elsewhere has to be able to tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asking: Option<Prompt>,
    /// The socket this engine serves what the run's sessions say on, while
    /// it serves one. An engine older than the socket names none, and a
    /// reader of such a run hears nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<PathBuf>,
}

mod write;
pub use write::{register, ProcessRegistry};

/// Where a run's registry lives: `run.dir/scratch/engine.json`.
pub fn registry_path(run_dir: &Path) -> PathBuf {
    run_dir.join(registry_file().as_path())
}

/// The registry's place under the run directory — what a finding about
/// it names, since a location is relative to the run and never to this
/// host.
pub fn registry_file() -> RelativePath {
    RelativePath::of([crate::run_dir::SCRATCH_DIR, REGISTRY_NAME])
}

/// The registry's file name under the run's scratch.
const REGISTRY_NAME: &str = "engine.json";

/// A run's registry, as this binary reads it.
///
/// Three answers, not two: no registry at all is a run no live engine
/// ever wrote one for, a registry that reads is what it says, and a
/// registry that does not read is a fact about this run worth saying —
/// a caller that reported it as absent would be telling a person the
/// engine was never there.
pub enum Registry {
    /// No registry: nothing wrote one, or a terminal deleted it.
    Absent,
    /// The registry, and whatever a newer binary wrote beside it.
    Read(Box<yunta_core::persisted::PersistedDoc<EngineProcessFile>>),
    /// A registry this binary cannot make sense of.
    Corrupt(yunta_core::persisted::PersistedError),
}

pub fn read_registry(run_dir: &Path) -> Registry {
    let Ok(bytes) = std::fs::read(registry_path(run_dir)) else {
        return Registry::Absent;
    };
    match yunta_core::persisted::PersistedDoc::read(&bytes) {
        Ok(registry) => Registry::Read(Box::new(registry)),
        Err(error) => Registry::Corrupt(error),
    }
}

/// The groups among `groups` that run, or that cannot be inspected: what a
/// registry keeps of a group once the engine that registered it is gone. A
/// group is dropped only when no member of it runs.
pub fn running_groups(groups: &[Pid]) -> Vec<Pid> {
    groups
        .iter()
        .copied()
        .filter(|group| group_runs(*group).unwrap_or(true))
        .collect()
}

impl EngineProcessFile {
    /// Whether the engine this registry names is still the process that
    /// wrote it.
    ///
    /// A live pid is not enough. The engine may have died and the host
    /// may have handed its number to something else entirely, so the
    /// answer is the one the isolation lock asks of its own holder: the
    /// process has to have started no later than the registry says the
    /// engine did.
    pub fn liveness(&self, probe: &dyn crate::lock::OwnerProbe) -> Liveness {
        crate::lock::holder_state(
            &crate::lock::LockOwner {
                schema_version: <crate::lock::LockOwner as Persisted>::SCHEMA_VERSION,
                pid: self.engine_pid,
                started_at: self.started_at,
            },
            probe,
        )
    }
}

/// What the registry under `run_dir` says about the process driving the
/// run: one read of the file and one probe of its process, so a reading
/// never joins what two instants said.
///
/// Only [`Engine::Dead`] is proof that nothing drives the run: an absent
/// registry is also what a run being handed between processes looks like,
/// and a reader that called that stalled would be wrong for the instant it
/// lasts.
pub fn read_engine(run_dir: &Path, probe: &dyn crate::lock::OwnerProbe) -> Engine {
    match read_registry(run_dir) {
        Registry::Absent => Engine::Unrecorded,
        Registry::Corrupt(_) => Engine::Unknown,
        Registry::Read(registry) => match registry.doc.liveness(probe) {
            Liveness::Alive => Engine::Alive {
                pid: registry.doc.engine_pid,
                prompt: registry.doc.asking.clone(),
                words: registry.doc.words.clone(),
            },
            Liveness::Dead => Engine::Dead,
            Liveness::Unknown => Engine::Unknown,
        },
    }
}

/// The pid of a spawned child, or `None` once it has been reaped.
pub fn child_pid(child: &tokio::process::Child) -> Option<Pid> {
    child.id().and_then(|id| Pid::try_from(id).ok())
}

#[cfg(test)]
mod tests;
