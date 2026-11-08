use super::*;
use crate::lock::OwnerProbe;
use yunta_core::NodeId;
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
    assert_eq!(
        read_engine(run.path(), &host),
        Engine::Alive {
            pid: Pid::try_from(1u32).unwrap(),
            prompt: None,
            words: None,
        }
    );
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
    assert_eq!(read_engine(run.path(), &gone), Engine::Dead);
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
    assert_eq!(read_engine(run.path(), &stranger), Engine::Dead);
    let engine = Host {
        alive: Liveness::Alive,
        started: Some(recorded),
    };
    assert_eq!(
        read_engine(run.path(), &engine),
        Engine::Alive {
            pid: Pid::try_from(4321u32).expect("a pid"),
            prompt: None,
            words: None,
        }
    );
}

#[test]
fn a_run_with_no_engine_record_is_never_called_dead() {
    let run = tempfile::tempdir().unwrap();
    let gone = Host {
        alive: Liveness::Dead,
        started: None,
    };
    assert_eq!(read_engine(run.path(), &gone), Engine::Unrecorded);
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
    assert_eq!(read_engine(run.path(), &gone), Engine::Unknown);
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

/// The reading of the registry names the socket a live engine serves, and
/// none for a registry that names none, as an older engine's does, or for an
/// engine that is gone.
#[test]
fn the_registry_reading_names_the_socket_only_while_its_engine_lives() {
    let run = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(registry_path(run.path()).parent().unwrap()).unwrap();
    let started = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let alive = Host {
        alive: Liveness::Alive,
        started: Some(started),
    };
    let registry = ProcessRegistry::create(
        run.path(),
        Pid::try_from(1u32).unwrap(),
        &alive,
        &AtClock(started),
    )
    .unwrap();
    assert_eq!(read_engine(run.path(), &alive).words(), None);

    // The field as an engine that serves its socket writes it.
    let path = registry_path(run.path());
    let mut file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    file["words"] = "/tmp/yunta-501/run.sock".into();
    std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
    assert_eq!(
        read_engine(run.path(), &alive).words(),
        Some(Path::new("/tmp/yunta-501/run.sock"))
    );
    let gone = Host {
        alive: Liveness::Dead,
        started: None,
    };
    assert_eq!(read_engine(run.path(), &gone).words(), None);
    assert_eq!(Engine::this(Pid::try_from(1u32).unwrap()).words(), None);
    drop(registry);
}

/// What an engine asking a person at its terminal writes into `engine.json`
/// is the node it asks about and since when, as two fields, and a reading of
/// the file returns the question it wrote. A reader elsewhere, of any
/// version, finds the question under these names.
#[test]
fn an_engine_asking_writes_the_node_and_the_instant_and_reads_them_back() {
    let run = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(registry_path(run.path()).parent().unwrap()).unwrap();
    let started = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let alive = Host {
        alive: Liveness::Alive,
        started: Some(started),
    };
    let pid = Pid::try_from(1u32).unwrap();
    let registry = ProcessRegistry::create(run.path(), pid, &alive, &AtClock(started)).unwrap();
    let since = started + chrono::Duration::seconds(5);
    registry.asking(Some(Prompt {
        node: Some(NodeId::from("approve")),
        since,
    }));

    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(registry_path(run.path())).unwrap()).unwrap();
    assert_eq!(
        file["asking"],
        serde_json::json!({"node": "approve", "since": "2023-11-14T22:13:25Z"})
    );
    assert_eq!(
        read_engine(run.path(), &alive),
        Engine::Alive {
            pid,
            prompt: Some(Prompt {
                node: Some(NodeId::from("approve")),
                since,
            }),
            words: None,
        }
    );
    drop(registry);
}
