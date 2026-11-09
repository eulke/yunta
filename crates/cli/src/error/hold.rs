//! A refused claim on a run, in this border's vocabulary: the one place that reads
//! a [`LockError`] for a person. The lock says who holds a file; a reader wants to
//! know who drives the run, and what to do about it.

use std::path::Path;

use chrono::{DateTime, Utc};
use yunta_core::process::signal::Liveness;
use yunta_core::RunId;
use yunta_engine::lock::{LockError, SystemProbe};

use super::CliError;
use yunta_render::refusal::Refused;

impl CliError {
    /// Who drives the run when that can be told, and the lock's file when only a
    /// person can tell. Any other failure of the lock is the run's own. `at` is
    /// the instant the refusal reads the run's registry at.
    pub fn refused_hold(
        run_id: &RunId,
        run_dir: &Path,
        refusal: LockError,
        at: DateTime<Utc>,
    ) -> Self {
        let unclear = |lock, pid| {
            CliError::from(Refused::HoldUnclear {
                run: run_id.clone(),
                lock,
                pid,
            })
        };
        let driven = |pid| match yunta_engine::read_engine(run_dir, &SystemProbe).asking(at) {
            Some(asking) => CliError::from(Refused::Asking(asking)),
            None => CliError::from(Refused::Moving {
                run: run_id.clone(),
                pid,
            }),
        };
        match refusal {
            LockError::Held {
                lock_path,
                owner,
                liveness: Liveness::Unknown,
            } => unclear(lock_path, Some(owner.pid)),
            LockError::Held { owner, .. }
            | LockError::Timeout {
                owner: Some(owner), ..
            } => driven(owner.pid),
            LockError::Unreadable { lock_path } | LockError::Timeout { lock_path, .. } => {
                unclear(lock_path, None)
            }
            other => yunta_engine::RunError::from(other).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::DateTime;
    use yunta_core::Pid;
    use yunta_engine::lock::LockOwner;

    use super::*;
    use yunta_render::command::Reader;

    const WHOLE_ID: &str = "01M6NQR1YQ3A33XN7XYM1AYC07";
    const HANDLE: &str = "1AYC07";

    /// A directory with no registry: nobody is asking at a terminal there.
    fn run_dir() -> &'static Path {
        Path::new("/nowhere")
    }

    fn lock_path() -> PathBuf {
        run_dir().join("engine.lock")
    }

    fn owner() -> LockOwner {
        LockOwner {
            schema_version: 1,
            pid: Pid::try_from(91930_u32).unwrap(),
            started_at: DateTime::UNIX_EPOCH,
        }
    }

    fn refused(refusal: LockError) -> CliError {
        CliError::refused_hold(
            &RunId::from_static(WHOLE_ID),
            run_dir(),
            refusal,
            DateTime::UNIX_EPOCH,
        )
    }

    #[test]
    fn a_live_holder_is_who_drives_the_run() {
        let error = refused(LockError::Held {
            lock_path: lock_path(),
            owner: owner(),
            liveness: Liveness::Alive,
        });
        assert_eq!(
            error.said_to(Reader::Person),
            format!("run {HANDLE} is already moving: pid 91930 drives it")
        );
        assert_eq!(
            error.said_to(Reader::Agent),
            format!("run {WHOLE_ID} is already moving: pid 91930 drives it")
        );
    }

    /// A parked run's next engine waited for the one that parked it, which did not
    /// leave: it is still there, so it is still who drives the run.
    #[test]
    fn a_holder_that_outlasts_the_wait_is_who_drives_the_run() {
        let error = refused(LockError::Timeout {
            lock_path: lock_path(),
            owner: Some(owner()),
        });
        assert!(
            matches!(
                error,
                CliError::Refused(yunta_render::refusal::Refused::Moving { .. })
            ),
            "{}",
            error.said_to(Reader::Person)
        );
    }

    #[test]
    fn a_holder_this_user_cannot_signal_is_named_beside_its_file() {
        let error = refused(LockError::Held {
            lock_path: lock_path(),
            owner: owner(),
            liveness: Liveness::Unknown,
        });
        let said = error.said_to(Reader::Agent);
        assert!(
            said.starts_with(&format!(
                "run {WHOLE_ID} is held by pid 91930, a process this user cannot signal \
                 (`/nowhere/engine.lock`)"
            )),
            "{said}"
        );
    }

    #[test]
    fn a_file_that_names_nobody_is_named_however_it_was_met() {
        for refusal in [
            LockError::Unreadable {
                lock_path: lock_path(),
            },
            LockError::Timeout {
                lock_path: lock_path(),
                owner: None,
            },
        ] {
            let said = refused(refusal).said_to(Reader::Person);
            assert!(
                said.starts_with(&format!(
                    "run {HANDLE} has an engine lock with no readable owner \
                     (`/nowhere/engine.lock`)"
                )),
                "{said}"
            );
        }
    }

    #[test]
    fn a_stopped_wait_is_the_runs_own_cancellation() {
        let error = refused(LockError::Cancelled {
            lock_path: lock_path(),
        });
        assert!(
            matches!(error, CliError::Run(yunta_engine::RunError::Cancelled)),
            "{}",
            error.said_to(Reader::Person)
        );
    }
}
