//! The writer of a run's registry file: the one handle the run holds on
//! it, and what a terminal deletes once the run is over.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use yunta_core::durable::{self, Durability};
use yunta_core::persisted::Persisted;
use yunta_core::port::RunToolsEndpoint;
use yunta_core::Pid;

use super::{read_registry, registry_path, running_groups, Asked, EngineProcessFile, Registry};

/// Handle the run's imperative shell holds for the registry file.
pub struct ProcessRegistry {
    path: PathBuf,
    state: Mutex<EngineProcessFile>,
}

impl ProcessRegistry {
    /// Writes the initial file for this invocation. An earlier
    /// invocation's file is replaced — the new engine owns the run now —
    /// but the groups it still names and that still run stay registered:
    /// they are the run's processes, and this engine answers for them.
    pub fn create(
        run_dir: &Path,
        engine_pid: Pid,
        started_at: DateTime<Utc>,
    ) -> std::io::Result<ProcessRegistry> {
        let process_groups = match read_registry(run_dir) {
            Registry::Read(earlier) => running_groups(&earlier.doc.process_groups),
            Registry::Absent | Registry::Corrupt(_) => Vec::new(),
        };
        let state = EngineProcessFile {
            schema_version: <EngineProcessFile as Persisted>::SCHEMA_VERSION,
            engine_pid,
            started_at,
            process_groups,
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
    /// credential nobody should still be able to read. The exception is a
    /// group the engine kept, because it could not stop it, that still
    /// runs: the file stays, naming only that group, and the question this
    /// engine asked a person is gone with it.
    pub fn clear(&self) {
        // blocking: the terminal's last act, on the thread that reached
        // it, after the log is closed and nothing is left to schedule.
        let kept = {
            let mut state = lock(&self.state);
            state.asking = None;
            state.process_groups = running_groups(&state.process_groups);
            !state.process_groups.is_empty()
        };
        if kept {
            if let Err(e) = self.commit(|_| {}) {
                tracing::warn!(error = %e, "failed to keep a running process group in engine.json at run terminal");
            }
        } else {
            delete_registry(&self.path);
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

/// Deletes the registry file, where nothing is left for it to name.
fn delete_registry(path: &Path) {
    // blocking: see `ProcessRegistry::clear`.
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %e, "failed to delete engine.json at run terminal");
        }
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
    /// Set by [`PgidRegistration::keep`], which leaves the group registered.
    kept: bool,
}

impl PgidRegistration<'_> {
    /// Leaves the group registered after this guard is gone. The engine could
    /// not stop the group, so it may still run: the registry keeps it past the
    /// engine's exit while it runs, and a separate `yunta close` still finds it.
    pub fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for PgidRegistration<'_> {
    fn drop(&mut self) {
        if !self.kept {
            self.registry.remove(self.pgid);
        }
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
            Some(PgidRegistration {
                registry,
                pgid,
                kept: false,
            })
        }
        _ => None,
    }
}

/// The file dies with the invocation that wrote it: every `execute_run`
/// exit — terminal, pause, or error — drops the registry and deletes the
/// file, except for the kept groups that still run (see `clear`). A crash
/// (SIGKILL) skips this, on purpose: the leftover
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_registry::{read_registry, registry_path, Registry};

    /// A registry created the way a run creates its own: the scratch
    /// directory first, then the file.
    fn registry_in(run_dir: &Path) -> ProcessRegistry {
        std::fs::create_dir_all(registry_path(run_dir).parent().expect("a parent")).unwrap();
        let started = DateTime::from_timestamp(1_700_000_000, 0).expect("a timestamp");
        ProcessRegistry::create(run_dir, Pid::try_from(1u32).expect("a pid"), started).unwrap()
    }

    /// The groups the registry file holds, as a reader of the file sees them.
    fn groups_on_disk(run_dir: &Path) -> Vec<Pid> {
        let Registry::Read(registry) = read_registry(run_dir) else {
            panic!("the registry file does not read")
        };
        registry.doc.process_groups.clone()
    }

    /// A process group that runs until its leader is killed: a `sleep` leads
    /// it, so the group has a member that runs.
    fn running_group() -> std::process::Child {
        use std::os::unix::process::CommandExt;
        std::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap()
    }

    #[test]
    fn a_kept_registration_leaves_its_group_registered_once_its_guard_is_gone() {
        let run = tempfile::tempdir().unwrap();
        let registry = registry_in(run.path());
        let group = Pid::try_from(77u32).expect("a pid");
        register(Some(&registry), Some(group))
            .expect("a registration")
            .keep();
        assert_eq!(groups_on_disk(run.path()), vec![group]);
    }

    #[test]
    fn a_dropped_registration_removes_its_group_from_the_registry() {
        let run = tempfile::tempdir().unwrap();
        let registry = registry_in(run.path());
        let group = Pid::try_from(77u32).expect("a pid");
        drop(register(Some(&registry), Some(group)));
        assert!(groups_on_disk(run.path()).is_empty());
    }

    #[test]
    fn a_kept_group_that_still_runs_outlives_the_engine_that_kept_it() {
        let run = tempfile::tempdir().unwrap();
        let registry = registry_in(run.path());
        // The group runs, so it stands in for a group the engine could not stop.
        let mut leader = running_group();
        let group = Pid::try_from(leader.id()).expect("a pid");
        register(Some(&registry), Some(group))
            .expect("a registration")
            .keep();
        drop(registry);
        assert_eq!(groups_on_disk(run.path()), vec![group]);
        leader.kill().expect("the sleep is killed");
        leader.wait().expect("the sleep is reaped");
    }

    #[test]
    fn a_kept_group_that_has_exited_leaves_no_registry_behind() {
        let run = tempfile::tempdir().unwrap();
        let registry = registry_in(run.path());
        // No process has this id: the group is gone before the engine exits.
        let group = Pid::try_from(2_000_000_000u32).expect("a pid");
        register(Some(&registry), Some(group))
            .expect("a registration")
            .keep();
        drop(registry);
        assert!(!registry_path(run.path()).exists());
    }

    #[test]
    fn a_later_invocation_keeps_the_running_groups_an_earlier_one_kept() {
        let run = tempfile::tempdir().unwrap();
        let earlier = registry_in(run.path());
        let mut leader = running_group();
        let group = Pid::try_from(leader.id()).expect("a pid");
        register(Some(&earlier), Some(group))
            .expect("a registration")
            .keep();
        drop(earlier);
        let later = registry_in(run.path());
        assert_eq!(groups_on_disk(run.path()), vec![group]);
        drop(later);
        leader.kill().expect("the sleep is killed");
        leader.wait().expect("the sleep is reaped");
    }
}
