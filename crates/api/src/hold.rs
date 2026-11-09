//! See [`crate`]. A run held by this process, ready to be driven here: one
//! created a moment ago, or a parked run taken back. A run is held through the
//! lock on its directory, which names the process driving it, so a run has one
//! engine. Whoever else holds it is refused as what holds it is: another
//! engine moving it, one asking a person at its own terminal, or a lock
//! nobody here can tell from a leftover.
//!
//! Taking a parked run back refuses first what does not clear by waiting — a
//! run this binary cannot serve, one another engine drives — then waits out
//! the limit reset it is paused on, then claims it, and only then takes its
//! checkout: a resume refused at any step takes nothing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use yunta_core::events::{EventPayload, SessionEvent};
use yunta_core::process::signal::Liveness;
use yunta_core::refusal::Unservable;
use yunta_core::step::Arg;
use yunta_core::{Clock, Isolation, Manifest, RunId};
use yunta_engine::lock::{Contention, LockError, SystemProbe};
use yunta_engine::RunHold;
use yunta_render::document::PreRunWarnings;
use yunta_render::refusal::Refused;
use yunta_replay::history::PriorEstimation;
use yunta_replay::RunState;

use crate::{CreatedRun, Error, OpenedRun, Project, Remark, Remarks, Stop};

/// A run this process holds, ready to be driven here.
///
/// Not the run as it is opened to be read: a held run is one this process
/// alone may move.
pub struct HeldRun<'p> {
    pub(crate) project: &'p Project,
    pub(crate) hold: RunHold,
    pub id: RunId,
    /// The directory the run keeps its manifest, its lock and its documents
    /// in.
    pub dir: PathBuf,
    /// The tree the run works in on this wake.
    pub worktree: PathBuf,
    pub manifest: Manifest,
    /// What this workflow's past runs cost: what a run's creation read, or,
    /// for a run parked on its token budget, the sample its creation read.
    pub prior: Option<PriorEstimation>,
    /// Every caution said before the first token: said to whoever creates a
    /// run, so none for a run taken back.
    pub warnings: PreRunWarnings,
}

impl<'p> CreatedRun<'p> {
    /// The created run, held by this process: anyone else holding it is
    /// refused at once.
    pub async fn hold(self, stop: &Stop) -> Result<HeldRun<'p>, Error> {
        let clock = self.project.workspace.clock.as_ref();
        let hold = yunta_engine::hold_run(&self.dir, Contention::Refuse, clock, stop.work())
            .await
            .map_err(yunta_engine::RunError::from)?;
        Ok(HeldRun {
            project: self.project,
            hold,
            id: self.id,
            dir: self.dir,
            worktree: self.worktree,
            manifest: self.manifest,
            prior: self.prior,
            warnings: self.warnings,
        })
    }
}

impl<'p> OpenedRun<'p> {
    /// The parked run, taken back by this process to be driven again: refused
    /// when this binary cannot serve it or another engine drives it, after
    /// the reset it is paused on, said through `remarks` while it lies ahead.
    /// `None` when the caller's stop ended a wait: the run stays parked, and
    /// nothing was taken.
    pub async fn take_back(
        self,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Option<HeldRun<'p>>, Error> {
        let project = self.project;
        let state = RunState::replay(&self.events);
        let manifest = &self.manifest.doc;
        // Resolved before any checkout is taken, so a refused resume takes no
        // checkout and writes nothing; the wake resolves the same redactor
        // again.
        yunta_engine::run_redactor(manifest, project.workspace.secrets.as_ref())?;
        if predates_frozen_adapter(manifest, &self.events) {
            return Err(Refused::Unservable(Unservable::MockBeforeFrozenAdapter).into());
        }
        // A run taken back from its log has no file of its own to check again.
        crate::machine::servable(manifest, state.run.mode(), Arg::Blank).await?;
        self.refuse_if_driven().await?;
        if !waited_out_reset(
            project.workspace.clock.as_ref(),
            &self.id,
            &state,
            remarks,
            stop,
        )
        .await
        {
            return Ok(None);
        }
        let Some(hold) = self.claimed(&state, stop).await? else {
            return Ok(None);
        };
        let bound = state.run.checkout().map(Path::to_path_buf);
        let named = project.run_tree(manifest, bound.as_deref(), &self.id, &project.workspace.dir);
        let worktree = project
            .still_or_again(manifest, &self.id, named, stop)
            .await?;
        let frame = self.frame_of(&state, project.workspace.clock.now());
        let prior = project.budget_prior(manifest, &frame, &self.events).await;
        Ok(Some(HeldRun {
            project,
            hold,
            id: self.id,
            dir: self.dir,
            worktree,
            manifest: self.manifest.doc,
            prior,
            warnings: PreRunWarnings::default(),
        }))
    }

    /// Refuses a run another engine holds, before anything waits for a reset
    /// or starts a process to drive it: what holds it now does not clear by
    /// waiting, and that process's own refusal would reach nobody. A live
    /// holder of a parked run passes, because the engine that parks a run is
    /// on its way out; a claim waits for it.
    ///
    /// Takes nothing, so it only says the run is free at this moment: the
    /// claim is what makes it so.
    pub async fn refuse_if_driven(&self) -> Result<(), Error> {
        let parked = RunState::replay(&self.events).run.paused().is_some();
        match yunta_engine::hold_refused(&self.dir).await {
            None => Ok(()),
            Some(LockError::Held {
                liveness: Liveness::Alive,
                ..
            }) if parked => Ok(()),
            Some(refusal) => Err(self.refused_hold(refusal)),
        }
    }

    /// Claims the run for this process, waiting for the engine that parked
    /// it to leave. `None` when the caller's stop ended that wait.
    async fn claimed(&self, state: &RunState, stop: &Stop) -> Result<Option<RunHold>, Error> {
        let contention = yunta_engine::contention_for(state);
        let clock = self.project.workspace.clock.as_ref();
        match yunta_engine::hold_run(&self.dir, contention, clock, stop.work()).await {
            Ok(hold) => Ok(Some(hold)),
            Err(LockError::Cancelled { .. }) => Ok(None),
            Err(refused) => Err(self.refused_hold(refused)),
        }
    }

    /// What a lock on this run that this process could not take is refused
    /// as.
    fn refused_hold(&self, refusal: LockError) -> Error {
        let now = self.project.workspace.clock.now();
        held_refusal(&self.id, &self.dir, refusal, now)
    }
}

/// What a lock on run `id`, whose directory is `dir`, that a process could not
/// take at `now` is refused as: an engine asking a person at its terminal,
/// another engine moving it, or a lock that names nobody this user can check.
/// A stopped wait is the caller's stop.
fn held_refusal(id: &RunId, dir: &Path, refusal: LockError, now: DateTime<Utc>) -> Error {
    let unclear = |lock, pid| {
        Error::from(Refused::HoldUnclear {
            run: id.clone(),
            lock,
            pid,
        })
    };
    let driven = |pid| match yunta_engine::read_engine(dir, &SystemProbe).asking(now) {
        Some(asking) => Error::from(Refused::Asking(asking)),
        None => Error::from(Refused::Moving {
            run: id.clone(),
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

impl Project {
    /// The checkout run `id` works in on this wake: the one its log names
    /// while it is still the run's, or else the one the project's pool finds
    /// for the run's branch — given back while the run was parked, or taken
    /// away. A run working in a person's checkout works where it always did.
    async fn still_or_again(
        &self,
        manifest: &Manifest,
        id: &RunId,
        named: PathBuf,
        stop: &Stop,
    ) -> Result<PathBuf, Error> {
        if manifest.isolation != Isolation::Worktree {
            return Ok(named);
        }
        let pool = self.pool_of(manifest, &self.runs_root.join(id.as_str()));
        let supervision = self.workspace.supervision(stop);
        Ok(pool.wake_checkout(id, &named, supervision).await?)
    }

    /// The pool of checkouts run `manifest` takes its trees from, for the run
    /// whose directory is `dir`, reached from the project's checkout and
    /// reading time as the workspace does.
    pub(crate) fn pool_of(
        &self,
        manifest: &Manifest,
        dir: &Path,
    ) -> std::sync::Arc<yunta_engine::CheckoutPool> {
        yunta_engine::CheckoutPool::of_run(
            &self.worktrees_root_for(manifest),
            &self.workspace.dir,
            dir,
            manifest,
            std::sync::Arc::clone(&self.workspace.clock),
        )
    }
}

/// Whether a manifest with no adapter, whose log holds a session the mock
/// opened, predates frozen adapters: its fixture was never kept.
fn predates_frozen_adapter(
    manifest: &Manifest,
    events: &[yunta_core::events::StoredEvent],
) -> bool {
    manifest.adapter.is_none()
        && events.iter().any(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(opened)))
                    if opened.session_id.as_str().starts_with(yunta_adapters::MOCK_SESSION_PREFIX)
            )
        })
}

/// Waits, when run `id` is paused on a limit's reset, until that reset has
/// come, said through `remarks` while it lies ahead. `false` when the
/// caller's stop ended the wait.
async fn waited_out_reset(
    clock: &dyn Clock,
    id: &RunId,
    state: &RunState,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> bool {
    let (Some(at), Some((reason, _))) = (crate::detach::reset_at(state), state.run.paused()) else {
        return true;
    };
    if !time_until(clock, at).is_zero() {
        remarks.remark(Remark::WaitingForReset {
            run: id.clone(),
            reason: reason.clone(),
        });
    }
    waited_until(clock, at, stop).await
}

/// Waits until `at` before anything is taken. The caller's stop ends the wait
/// first and returns `false`; a past instant returns at once.
async fn waited_until(clock: &dyn Clock, at: DateTime<Utc>, stop: &Stop) -> bool {
    waited_out(|| time_until(clock, at), stop).await
}

/// Sleeps until `left` reads nothing left, asking it again after each sleep:
/// a timer runs on the monotonic clock, which drifts from the wall clock
/// `left` reads over a long wait, so one sleep can end before the instant.
async fn waited_out(left: impl Fn() -> Duration, stop: &Stop) -> bool {
    loop {
        let remaining = left();
        if remaining.is_zero() {
            return true;
        }
        let deadline = tokio::time::Instant::now() + remaining;
        tokio::select! {
            () = yunta_engine::process::wait_for_deadline(Some(deadline)) => {}
            () = stop.work().cancelled() => return false,
        }
    }
}

/// How long until `at` by `clock`: zero once it has passed.
pub(crate) fn time_until(clock: &dyn Clock, at: DateTime<Utc>) -> Duration {
    (at - clock.now()).to_std().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use yunta_core::Pid;
    use yunta_engine::lock::LockOwner;
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

    fn refused(refusal: LockError) -> Error {
        held_refusal(
            &RunId::from_static(WHOLE_ID),
            run_dir(),
            refusal,
            DateTime::UNIX_EPOCH,
        )
    }

    fn said(error: &Error, reader: Reader) -> String {
        match error {
            Error::Refused(refused) => refused.said(reader),
            other => other.to_string(),
        }
    }

    #[test]
    fn a_live_holder_is_who_drives_the_run() {
        let error = refused(LockError::Held {
            lock_path: lock_path(),
            owner: owner(),
            liveness: Liveness::Alive,
        });
        assert_eq!(
            said(&error, Reader::Person),
            format!("run {HANDLE} is already moving: pid 91930 drives it")
        );
        assert_eq!(
            said(&error, Reader::Agent),
            format!("run {WHOLE_ID} is already moving: pid 91930 drives it")
        );
    }

    /// A parked run's next engine waited for the one that parked it, which did
    /// not leave: it is still there, so it is still who drives the run.
    #[test]
    fn a_holder_that_outlasts_the_wait_is_who_drives_the_run() {
        let error = refused(LockError::Timeout {
            lock_path: lock_path(),
            owner: Some(owner()),
        });
        assert!(
            matches!(error, Error::Refused(Refused::Moving { .. })),
            "{}",
            said(&error, Reader::Person)
        );
    }

    #[test]
    fn a_holder_this_user_cannot_signal_is_named_beside_its_file() {
        let error = refused(LockError::Held {
            lock_path: lock_path(),
            owner: owner(),
            liveness: Liveness::Unknown,
        });
        let said = said(&error, Reader::Agent);
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
            let said = said(&refused(refusal), Reader::Person);
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
    fn a_stopped_wait_is_the_callers_stop() {
        let error = refused(LockError::Cancelled {
            lock_path: lock_path(),
        });
        assert!(
            matches!(error, Error::Stopped),
            "{}",
            said(&error, Reader::Person)
        );
    }

    #[tokio::test]
    async fn a_stop_ends_the_wait_for_a_reset_still_ahead() {
        let stop = Stop::new();
        stop.stop();
        // A reset no test run reaches: the wait can only end by the stop.
        let ahead = Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).unwrap();
        assert!(!waited_until(&yunta_testkit_core::FixedClock, ahead, &stop).await);
    }

    #[tokio::test]
    async fn a_wait_whose_sleep_ends_before_the_wall_clock_sleeps_again() {
        // The wall clock reads a moment left after the first sleep, as it does
        // when the timer ran ahead of it, and nothing left after the next.
        let asked = std::cell::Cell::new(0);
        let left = || {
            asked.set(asked.get() + 1);
            match asked.get() {
                1 | 2 => Duration::from_millis(1),
                _ => Duration::ZERO,
            }
        };
        assert!(waited_out(left, &Stop::new()).await);
        assert_eq!(
            asked.get(),
            3,
            "the wait asks the wall clock until it has come"
        );
    }

    #[tokio::test]
    async fn a_reset_already_past_is_not_waited_for() {
        let past = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        assert!(waited_until(&yunta_testkit_core::FixedClock, past, &Stop::new()).await);
    }
}
