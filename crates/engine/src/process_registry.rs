//! Per-run process registry: `run.dir/scratch/engine.json`,
//! the one thing that lets a *separate* process — `yunta cancel`, a
//! future `--detach` supervisor — find and signal a live run's process
//! tree so cancellation can always tear the whole tree down, not just
//! the top process. Scratch, deliberately: it is ephemeral process
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
use yunta_core::{Pid, RelativePath};

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

/// The pid of a spawned child, or `None` once it has been reaped.
pub fn child_pid(child: &tokio::process::Child) -> Option<Pid> {
    child.id().and_then(|id| Pid::try_from(id).ok())
}

fn lock(state: &Mutex<EngineProcessFile>) -> std::sync::MutexGuard<'_, EngineProcessFile> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
