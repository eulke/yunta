//! Per-run process registry: `run.dir/scratch/engine.json`,
//! the one thing that lets a *separate* process — `yunta cancel`, which
//! shares no memory with the run it stops — find and signal a live run's
//! process tree so cancellation can always tear the whole tree down, not
//! just the top process. Scratch, deliberately: it is ephemeral process
//! state, not an artifact and not event-log truth; it is written when
//! `execute_run` starts, updated as sessions/hooks/executors spawn and
//! close, and deleted at every terminal.
//!
//! Registration is best-effort bookkeeping around processes that are
//! already owned and killed by the in-process paths: a failed write
//! here degrades to a `tracing` warning, never to a failed run — but it
//! degrades *loudly*, never silently.

use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use yunta_core::persisted::Persisted;
use yunta_core::process::signal::Liveness;
use yunta_core::{NodeId, Pid, RelativePath};

impl Persisted for EngineProcessFile {
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
    /// When this engine started, as its own injected clock read it —
    /// what tells a live pid apart from a number the host handed to
    /// something else after a crash.
    pub started_at: DateTime<Utc>,
    /// Process-group ids of live sessions/hooks/executors — what a
    /// post-crash `cancel` kills directly when `engine_pid` is gone.
    pub process_groups: Vec<Pid>,
    /// The person this engine is asking at its terminal, while it asks:
    /// a question asked live leaves nothing on the log until it is
    /// answered, and a reader elsewhere has to be able to tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asking: Option<Asked>,
}

/// What an engine asks a person at its terminal: the node that asks,
/// when one does, and since when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asked {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub since: DateTime<Utc>,
}

/// Handle the run's imperative shell holds for the registry file.
pub struct ProcessRegistry {
    path: PathBuf,
    state: Mutex<EngineProcessFile>,
}

impl ProcessRegistry {
    /// Writes the initial file for this invocation. An earlier
    /// invocation's file (crash leftovers) is simply overwritten — the
    /// new engine owns the run now.
    pub fn create(
        run_dir: &Path,
        engine_pid: Pid,
        started_at: DateTime<Utc>,
    ) -> std::io::Result<ProcessRegistry> {
        let state = EngineProcessFile {
            schema_version: <EngineProcessFile as Persisted>::SCHEMA_VERSION,
            engine_pid,
            started_at,
            process_groups: Vec::new(),
            asking: None,
        };
        let registry = ProcessRegistry {
            path: registry_path(run_dir),
            state: Mutex::new(state),
        };
        registry.persist()?;
        Ok(registry)
    }

    /// Registers a spawned process group. Failures warn, never fail the
    /// run — see the module doc.
    pub fn add(&self, pgid: Pid) {
        let mut state = lock(&self.state);
        if !state.process_groups.contains(&pgid) {
            state.process_groups.push(pgid);
        }
        drop(state);
        if let Err(e) = self.persist() {
            tracing::warn!(pgid = %pgid, error = %e, "failed to register a process group in engine.json");
        }
    }

    /// Unregisters a closed process group.
    pub fn remove(&self, pgid: Pid) {
        let mut state = lock(&self.state);
        state.process_groups.retain(|existing| *existing != pgid);
        drop(state);
        if let Err(e) = self.persist() {
            tracing::warn!(pgid = %pgid, error = %e, "failed to unregister a process group in engine.json");
        }
    }

    /// Records that this engine is asking a person at its terminal, or,
    /// with `None`, that it no longer is. Failures warn, never fail the
    /// run.
    pub fn asking(&self, asked: Option<Asked>) {
        lock(&self.state).asking = asked;
        if let Err(e) = self.persist() {
            tracing::warn!(error = %e, "failed to record a live question in engine.json");
        }
    }

    /// Deletes what the run's scratch directory holds about live
    /// processes — the registry itself, and the MCP config a session's
    /// adapter was pointed at, which carries that session's bearer.
    ///
    /// The run reached a terminal: there is nothing left for an outside
    /// process to signal, and a credential nobody can use is a
    /// credential nobody should still be able to read.
    pub fn clear(&self) {
        // blocking: the terminal's last act, on the thread that reached
        // it, after the log is closed and nothing is left to schedule.
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "failed to delete engine.json at run terminal");
            }
        }
        if let Some(scratch) = self.path.parent() {
            delete_credentials_under(scratch);
        }
    }

    fn persist(&self) -> std::io::Result<()> {
        let state = lock(&self.state);
        let json = yunta_core::persisted::PersistedDoc::of(state.clone())
            .write()
            .map_err(std::io::Error::other)?;
        drop(state);
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// Deletes every `mcp.json` under `scratch/`: one per session that held
/// the run's tools, each carrying that session's bearer. A directory
/// that cannot be listed is left alone — the run already closed, and a
/// warning about a file nobody can reach helps no one.
fn delete_credentials_under(scratch: &Path) {
    // blocking: see `clear`.
    let Ok(entries) = std::fs::read_dir(scratch) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => delete_credentials_under(&path),
            Ok(_) if path.file_name().is_some_and(|name| name == "mcp.json") => {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "failed to delete a session's tool credentials at run terminal"
                    );
                }
            }
            _ => {}
        }
    }
}

/// RAII registration: `add` on creation, `remove` on drop — one binding
/// at the spawn site covers every exit path of the spawning function,
/// cancellation and error included.
pub struct PgidRegistration<'a> {
    registry: &'a ProcessRegistry,
    pgid: Pid,
}

impl Drop for PgidRegistration<'_> {
    fn drop(&mut self) {
        self.registry.remove(self.pgid);
    }
}

/// Registers `pgid` for the guard's lifetime. `None` in either input —
/// no registry (degraded run), no pgid (mock session) — registers
/// nothing, so call sites never branch.
pub fn register<'a>(
    registry: Option<&'a ProcessRegistry>,
    pgid: Option<Pid>,
) -> Option<PgidRegistration<'a>> {
    match (registry, pgid) {
        (Some(registry), Some(pgid)) => {
            registry.add(pgid);
            Some(PgidRegistration { registry, pgid })
        }
        _ => None,
    }
}

/// The file dies with the invocation that wrote it: every `execute_run`
/// exit — terminal, pause, or error — drops the registry and deletes
/// the file. A crash (SIGKILL) skips this, on purpose: the leftover
/// file with its recorded pgids is exactly what a post-crash
/// `yunta cancel` cleans up from.
impl Drop for ProcessRegistry {
    fn drop(&mut self) {
        self.clear();
    }
}

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

/// Whether a live engine is driving a run, as far as the run's own
/// registry can prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLiveness {
    /// The process the registry names is the one that wrote it.
    Alive,
    /// The registry names a process that is gone, or a number the host
    /// has since given to another process: nothing drives the run.
    Dead,
    /// No registry. No engine is driving the run now — one deletes it on
    /// every way out, a pause included — or one is about to write it: a
    /// run handed to a detached process has none for an instant.
    Unrecorded,
    /// A registry this binary cannot read, or a process the host cannot
    /// tell about.
    Unknown,
}

/// What the registry under `run_dir` says about the engine driving it.
///
/// Only [`EngineLiveness::Dead`] is proof that nothing drives the run: an
/// absent registry is also what a run being handed between processes
/// looks like, and a reader that called that stalled would be wrong for
/// the instant it lasts.
pub fn engine_liveness(run_dir: &Path, probe: &dyn crate::lock::OwnerProbe) -> EngineLiveness {
    match read_registry(run_dir) {
        Registry::Absent => EngineLiveness::Unrecorded,
        Registry::Corrupt(_) => EngineLiveness::Unknown,
        Registry::Read(registry) => match registry.doc.liveness(probe) {
            Liveness::Alive => EngineLiveness::Alive,
            Liveness::Dead => EngineLiveness::Dead,
            Liveness::Unknown => EngineLiveness::Unknown,
        },
    }
}

/// A person a live engine is asking at its terminal: the node that asks,
/// since when, and the process whose terminal holds the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub node: Option<NodeId>,
    pub since: DateTime<Utc>,
    pub pid: Pid,
}

/// Whom the engine driving the run under `run_dir` is asking at its
/// terminal; `None` when it asks no one, or no live engine drives it.
pub fn engine_prompt(run_dir: &Path, probe: &dyn crate::lock::OwnerProbe) -> Option<Prompt> {
    let Registry::Read(registry) = read_registry(run_dir) else {
        return None;
    };
    if registry.doc.liveness(probe) != Liveness::Alive {
        return None;
    }
    let asked = registry.doc.asking.clone()?;
    Some(Prompt {
        node: asked.node,
        since: asked.since,
        pid: registry.doc.engine_pid,
    })
}

/// The pid of a spawned child, or `None` once it has been reaped.
pub fn child_pid(child: &tokio::process::Child) -> Option<Pid> {
    child.id().and_then(|id| Pid::try_from(id).ok())
}

fn lock(state: &Mutex<EngineProcessFile>) -> std::sync::MutexGuard<'_, EngineProcessFile> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::OwnerProbe;

    /// A host whose process table holds one process, started at a fixed
    /// instant, under every pid.
    struct Host {
        alive: Liveness,
        started: Option<DateTime<Utc>>,
    }

    impl OwnerProbe for Host {
        fn liveness(&self, _pid: Pid) -> Liveness {
            self.alive
        }
        fn started(&self, _pid: Pid) -> Option<DateTime<Utc>> {
            self.started
        }
    }

    /// A registry the way a crashed engine leaves one: written, and never
    /// cleared.
    fn left_behind(run_dir: &Path, started_at: DateTime<Utc>) {
        let pid = Pid::try_from(4321u32).expect("a pid");
        std::fs::create_dir_all(registry_path(run_dir).parent().expect("a parent")).unwrap();
        std::mem::forget(ProcessRegistry::create(run_dir, pid, started_at).expect("registry"));
    }

    #[test]
    fn an_engine_record_naming_a_process_that_exited_reads_as_dead() {
        let run = tempfile::tempdir().unwrap();
        left_behind(
            run.path(),
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        let gone = Host {
            alive: Liveness::Dead,
            started: None,
        };
        assert_eq!(engine_liveness(run.path(), &gone), EngineLiveness::Dead);
    }

    #[test]
    fn a_reused_pid_that_started_after_the_record_reads_as_dead() {
        let run = tempfile::tempdir().unwrap();
        let recorded = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        left_behind(run.path(), recorded);
        let stranger = Host {
            alive: Liveness::Alive,
            started: Some(recorded + chrono::Duration::hours(1)),
        };
        assert_eq!(engine_liveness(run.path(), &stranger), EngineLiveness::Dead);
        let engine = Host {
            alive: Liveness::Alive,
            started: Some(recorded),
        };
        assert_eq!(engine_liveness(run.path(), &engine), EngineLiveness::Alive);
    }

    #[test]
    fn a_run_with_no_engine_record_is_never_called_dead() {
        let run = tempfile::tempdir().unwrap();
        let gone = Host {
            alive: Liveness::Dead,
            started: None,
        };
        assert_eq!(
            engine_liveness(run.path(), &gone),
            EngineLiveness::Unrecorded
        );
    }

    #[test]
    fn a_registry_that_does_not_read_proves_nothing() {
        let run = tempfile::tempdir().unwrap();
        let path = registry_path(run.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        let gone = Host {
            alive: Liveness::Dead,
            started: None,
        };
        assert_eq!(engine_liveness(run.path(), &gone), EngineLiveness::Unknown);
    }
}
