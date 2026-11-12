use super::*;

/// A probe that says a process started at a fixed instant.
struct Table(Option<DateTime<Utc>>);

impl OwnerProbe for Table {
    fn liveness(&self, _pid: Pid) -> Liveness {
        Liveness::Alive
    }
    fn started(&self, _pid: Pid) -> Option<DateTime<Utc>> {
        self.0
    }
}

struct Frozen(DateTime<Utc>);

impl Clock for Frozen {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn at(year: i32) -> DateTime<Utc> {
    chrono::TimeZone::with_ymd_and_hms(&Utc, year, 1, 1, 0, 0, 0).unwrap()
}

/// The record states when the process started *by the same table*
/// `holder_state` reads. A run's clock answers about the run's
/// timeline: one frozen in the past would make every live holder
/// read as a stranger, and every lock stealable while its owner
/// works.
#[test]
fn a_lock_records_the_start_the_process_table_reports_not_the_clock() {
    let pid = Pid::current();
    let table = Table(Some(at(2020)));
    let owner = LockOwner {
        schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        pid,
        started_at: taken_at(pid, &table, &Frozen(at(2000))),
    };
    assert_eq!(owner.started_at, at(2020));
    assert_eq!(
        holder_state(&owner, &table),
        Liveness::Alive,
        "the process that took the lock still holds it"
    );
}

/// A host that cannot say when a process started leaves the clock as
/// the only answer — and the comparison it feeds cannot happen
/// either, so a live pid is taken at its word.
#[test]
fn a_host_with_no_process_table_falls_back_to_the_clock() {
    let pid = Pid::current();
    let blind = Table(None);
    let owner = LockOwner {
        schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        pid,
        started_at: taken_at(pid, &blind, &Frozen(at(2000))),
    };
    assert_eq!(owner.started_at, at(2000));
    assert_eq!(holder_state(&owner, &blind), Liveness::Alive);
}

/// A pid the host handed to something that started later is a
/// stranger, whatever the number says.
#[test]
fn a_pid_reused_by_a_later_process_reads_as_gone() {
    let pid = Pid::current();
    let owner = LockOwner {
        schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        pid,
        started_at: at(2020),
    };
    assert_eq!(holder_state(&owner, &Table(Some(at(2024)))), Liveness::Dead);
}

/// A probe that says a pid's process cannot be signalled, as another
/// user's cannot, and when it started.
struct Foreign(Option<DateTime<Utc>>);

impl OwnerProbe for Foreign {
    fn liveness(&self, _pid: Pid) -> Liveness {
        Liveness::Unknown
    }
    fn started(&self, _pid: Pid) -> Option<DateTime<Utc>> {
        self.0
    }
}

fn held_since(year: i32) -> LockOwner {
    LockOwner {
        schema_version: <LockOwner as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
        pid: Pid::current(),
        started_at: at(year),
    }
}

/// Another user got the pid after the lock was taken: the holder is gone,
/// and a process this one cannot signal does not keep its lock for good.
#[test]
fn a_pid_another_user_reused_after_the_lock_reads_as_gone() {
    assert_eq!(
        holder_state(&held_since(2020), &Foreign(Some(at(2024)))),
        Liveness::Dead
    );
}

/// Nothing shows that this process is not the holder, so it may be.
#[test]
fn a_process_that_cannot_be_signalled_and_could_be_the_holder_stays_unknown() {
    let owner = held_since(2020);
    assert_eq!(
        holder_state(&owner, &Foreign(Some(at(2019)))),
        Liveness::Unknown
    );
    assert_eq!(holder_state(&owner, &Foreign(None)), Liveness::Unknown);
}

#[tokio::test]
async fn reading_a_lock_answers_as_taking_it_would_and_takes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("held.lock");
    assert!(
        refusal_now(&path, &Table(None)).await.is_none(),
        "no file, no holder"
    );

    let holder = yunta_core::persisted::PersistedDoc::of(held_since(2020))
        .write()
        .unwrap();
    std::fs::write(&path, &holder).unwrap();
    assert!(matches!(
        refusal_now(&path, &Table(Some(at(2019)))).await,
        Some(LockError::Held {
            liveness: Liveness::Alive,
            ..
        })
    ));
    assert!(matches!(
        refusal_now(&path, &Foreign(None)).await,
        Some(LockError::Held {
            liveness: Liveness::Unknown,
            ..
        })
    ));
    assert!(
        refusal_now(&path, &Table(Some(at(2024)))).await.is_none(),
        "a later process with the holder's pid is not the holder"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        holder,
        "reading a lock neither removes nor writes it"
    );

    std::fs::write(&path, "not a holder").unwrap();
    assert!(matches!(
        refusal_now(&path, &Table(None)).await,
        Some(LockError::Unreadable { .. })
    ));
}
