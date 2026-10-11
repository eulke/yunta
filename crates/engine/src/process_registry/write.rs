//! The writer of a run's registry file: the one handle the run holds on
//! it, and what a terminal deletes once the run is over.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use yunta_core::durable::{self, Durability};
use yunta_core::persisted::Persisted;
use yunta_core::port::RunToolsEndpoint;
use yunta_core::Pid;

use super::{registry_path, Asked, EngineProcessFile};

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
        registry.commit(|_| {})?;
        Ok(registry)
    }

    /// Registers a spawned process group. Failures warn, never fail the
    /// run — see the module doc.
    pub fn add(&self, pgid: Pid) {
        let registered = self.commit(|state| {
            if !state.process_groups.contains(&pgid) {
                state.process_groups.push(pgid);
            }
        });
        if let Err(e) = registered {
            tracing::warn!(pgid = %pgid, error = %e, "failed to register a process group in engine.json");
        }
    }

    /// Unregisters a closed process group.
    pub fn remove(&self, pgid: Pid) {
        let unregistered = self.commit(|state| {
            state.process_groups.retain(|existing| *existing != pgid);
        });
        if let Err(e) = unregistered {
            tracing::warn!(pgid = %pgid, error = %e, "failed to unregister a process group in engine.json");
        }
    }

    /// Records that this engine is asking a person at its terminal, or,
    /// with `None`, that it no longer is. Failures warn, never fail the
    /// run.
    pub fn asking(&self, asked: Option<Asked>) {
        if let Err(e) = self.commit(|state| state.asking = asked) {
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

    /// Applies `change` and writes the whole state, both under the state's
    /// lock, so the file on disk is the snapshot of the last change made and
    /// never an older one that a racing writer renamed into place last.
    fn commit(&self, change: impl FnOnce(&mut EngineProcessFile)) -> std::io::Result<()> {
        let mut state = lock(&self.state);
        change(&mut state);
        let json = yunta_core::persisted::PersistedDoc::of(state.clone())
            .write()
            .map_err(std::io::Error::other)?;
        // blocking: one small file, replaced whole while the lock is held.
        durable::replace(&self.path, &json, Durability::Atomic)
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
            Ok(_)
                if path
                    .file_name()
                    .is_some_and(|name| name == RunToolsEndpoint::CONFIG_FILE) =>
            {
                // blocking: see `clear`.
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

fn lock(state: &Mutex<EngineProcessFile>) -> std::sync::MutexGuard<'_, EngineProcessFile> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
