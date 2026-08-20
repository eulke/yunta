//! Per-run process registry (DI-08): `run.dir/scratch/engine.json`,
//! the one thing that lets a *separate* process — `yunta cancel`, a
//! future `--detach` supervisor — find and signal a live run's process
//! tree (A4). Scratch, deliberately: it is ephemeral process state, not
//! an artifact and not event-log truth; it is written when `execute_run`
//! starts, updated as sessions/hooks/executors spawn and close, and
//! deleted at every terminal.
//!
//! Registration is best-effort bookkeeping around processes that are
//! already owned and killed by the in-process paths (T3.3/T4.6): a
//! failed write here degrades to a `tracing` warning, never to a failed
//! run — but it degrades *loudly*, never silently.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The file's whole content — small enough that every mutation rewrites
/// it atomically (tempfile + rename) rather than patching in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineProcessFile {
    /// The `yunta` process driving the run — signal it first (`yunta
    /// cancel` sends SIGINT here while it's alive, so the engine's own
    /// interrupt→kill path does the exterminating).
    pub engine_pid: u32,
    pub started_at: String,
    /// Process-group ids of live sessions/hooks/executors — what a
    /// post-crash `cancel` kills directly when `engine_pid` is gone.
    pub process_groups: Vec<u32>,
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
        engine_pid: u32,
        started_at: String,
    ) -> std::io::Result<ProcessRegistry> {
        let state = EngineProcessFile {
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
    pub fn add(&self, pgid: u32) {
        let mut state = lock(&self.state);
        if !state.process_groups.contains(&pgid) {
            state.process_groups.push(pgid);
        }
        drop(state);
        if let Err(e) = self.persist() {
            tracing::warn!(pgid, error = %e, "failed to register a process group in engine.json");
        }
    }

    /// Unregisters a closed process group.
    pub fn remove(&self, pgid: u32) {
        let mut state = lock(&self.state);
        state.process_groups.retain(|existing| *existing != pgid);
        drop(state);
        if let Err(e) = self.persist() {
            tracing::warn!(pgid, error = %e, "failed to unregister a process group in engine.json");
        }
    }

    /// Deletes the file — the run reached a terminal, there is nothing
    /// left for an outside process to signal.
    pub fn clear(&self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "failed to delete engine.json at run terminal");
            }
        }
    }

    fn persist(&self) -> std::io::Result<()> {
        let state = lock(&self.state);
        let json = serde_json::to_string_pretty(&*state).map_err(std::io::Error::other)?;
        drop(state);
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// RAII registration: `add` on creation, `remove` on drop — one binding
/// at the spawn site covers every exit path of the spawning function,
/// cancellation and error included.
pub struct PgidRegistration<'a> {
    registry: &'a ProcessRegistry,
    pgid: u32,
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
    pgid: Option<u32>,
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
    run_dir.join("scratch").join("engine.json")
}

/// Reads a run's registry, if one exists and parses — `None` covers
/// both "no live engine ever wrote one" and "unreadable", because the
/// caller's fallback is the same: work from the event log alone.
pub fn read_registry(run_dir: &Path) -> Option<EngineProcessFile> {
    let bytes = std::fs::read(registry_path(run_dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// `kill -0 <pid>` — POSIX liveness without `libc` or unsafe (D94 keeps
/// Windows out of scope). Lives in the imperative shell only.
pub fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn lock(state: &Mutex<EngineProcessFile>) -> std::sync::MutexGuard<'_, EngineProcessFile> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
