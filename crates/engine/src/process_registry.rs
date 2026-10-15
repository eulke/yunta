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
use yunta_core::{NodeId, Pid, RelativePath};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::OwnerProbe;
    use yunta_testkit_core::AtClock;

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

    /// A host that cannot say when a pid started: the clock is then the
    /// only reading, so a record says what the test chose.
    const SILENT: Host = Host {
        alive: Liveness::Unknown,
        started: None,
    };

    /// A registry the way a crashed engine leaves one: written, and never
    /// cleared.
    fn left_behind(run_dir: &Path, started_at: DateTime<Utc>) {
        let pid = Pid::try_from(4321u32).expect("a pid");
        std::fs::create_dir_all(registry_path(run_dir).parent().expect("a parent")).unwrap();
        std::mem::forget(
            ProcessRegistry::create(run_dir, pid, &SILENT, &AtClock(started_at)).expect("registry"),
        );
    }

    /// The registry states when its engine started as the process table
    /// reads it, not as the run's clock does: a reader compares the record
    /// against that table, so an engine whose clock is frozen still reads
    /// as the live engine it is.
    #[test]
    fn a_registry_under_a_frozen_clock_names_its_live_engine() {
        let run = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(registry_path(run.path()).parent().unwrap()).unwrap();
        let frozen = DateTime::from_timestamp(1_600_000_000, 0).unwrap();
        let host = Host {
            alive: Liveness::Alive,
            started: Some(DateTime::from_timestamp(1_700_000_000, 0).unwrap()),
        };
        let _registry = ProcessRegistry::create(
            run.path(),
            Pid::try_from(1u32).unwrap(),
            &host,
            &AtClock(frozen),
        )
        .unwrap();
        assert_eq!(engine_liveness(run.path(), &host), EngineLiveness::Alive);
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

    #[test]
    fn concurrent_registrations_leave_every_group_on_disk() {
        let run = tempfile::tempdir().unwrap();
        let started = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        // `create` writes under `scratch/`, which a run creates and a bare directory does not have.
        std::fs::create_dir_all(registry_path(run.path()).parent().unwrap()).unwrap();
        for round in 0..200u32 {
            let registry = ProcessRegistry::create(
                run.path(),
                Pid::try_from(1u32).unwrap(),
                &SILENT,
                &AtClock(started),
            )
            .unwrap();
            // Ids no process holds: a registry keeps the groups that still run
            // past its engine, so these are the ones that stay out of the next round.
            let groups: Vec<Pid> = (1..=16u32)
                .map(|n| Pid::try_from(2_000_000_000 + round * 16 + n).unwrap())
                .collect();
            std::thread::scope(|scope| {
                for &group in &groups {
                    let registry = &registry;
                    scope.spawn(move || registry.add(group));
                }
            });
            let Registry::Read(doc) = read_registry(run.path()) else {
                panic!("round {round}: unreadable")
            };
            assert_eq!(
                doc.doc.process_groups.len(),
                16,
                "round {round}: an older snapshot is on disk"
            );
            assert!(
                groups.iter().all(|g| doc.doc.process_groups.contains(g)),
                "round {round}: a group is lost"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_registry_file_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let run = tempfile::tempdir().unwrap();
        let started = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        std::fs::create_dir_all(registry_path(run.path()).parent().unwrap()).unwrap();
        let _registry = ProcessRegistry::create(
            run.path(),
            Pid::try_from(1u32).unwrap(),
            &SILENT,
            &AtClock(started),
        )
        .unwrap();
        let mode = std::fs::metadata(registry_path(run.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "engine.json is readable by others");
    }

    /// A group whose leader exited and was reaped is kept while a child the
    /// leader left in it runs: the leader's pid names no process, so the
    /// group is read through its members.
    #[cfg(unix)]
    #[test]
    fn running_groups_keeps_a_group_whose_leader_exited_while_a_child_runs() {
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        use yunta_core::process::signal::{signal_group, Signal};

        let mut leader = Command::new("sh")
            .args(["-c", "sleep 30 & exit 0"])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let group = Pid::try_from(leader.id()).unwrap();
        leader.wait().unwrap();

        assert_eq!(
            running_groups(&[group]),
            vec![group],
            "the sleep the leader left behind runs in the group"
        );
        // The sleep is killed, so the test leaves no process behind.
        signal_group(group, Signal::SIGKILL).unwrap();
    }
}
