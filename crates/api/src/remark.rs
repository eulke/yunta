//! See [`crate`]. What an operation says while it works, before it answers:
//! each remark reaches the caller through the [`Remarks`] it passed, in the
//! order the operation says it, and the caller says it then, in its own way.
//! A terminal writes each line as it comes; a program keeps them for its
//! document.

use std::path::PathBuf;
use std::time::Duration;

use yunta_core::caution::Caution;
use yunta_core::events::PauseReason;
use yunta_core::{Pid, RunId};
use yunta_replay::history::PriorEstimation;

/// One thing an operation says while it works.
///
/// Not a refusal, which ends the operation, nor a session's note, which a
/// run's agent says.
#[derive(Debug, Clone)]
pub enum Remark {
    /// Something a person can still act on before anything is spent.
    Caution(Caution),
    /// What the workflow's past runs cost, said before a run spends anything.
    Estimate(PriorEstimation),
    /// A lock on this checkout that a dead process left, which a run that
    /// isolates nothing takes over: whose it was.
    StaleLockTaken { pid: Pid },
    /// A run taken back while the limit reset it is paused on lies ahead:
    /// the run, and the pause its log recorded, which names the node and the
    /// instant.
    WaitingForReset { run: RunId, reason: PauseReason },
    /// The forge the config declares does not take the token it was given,
    /// so external gates fall back to the person: what it said.
    ForgeUnavailable { said: String },
    /// The live engine of a run a cancel stops, about to be interrupted: the
    /// run, and the engine's pid.
    SignallingEngine { run: RunId, engine: Pid },
    /// The caller stopped waiting on the engine a cancel interrupted, which
    /// stops the run on its own: the run.
    StoppedWaiting { run: RunId },
    /// The engine a cancel interrupted did not stop within `waited`, so its
    /// process groups are killed next: the run.
    Escalating { run: RunId, waited: Duration },
    /// A process group a run's registry names that is no run's own, so the
    /// registry is corrupt and the group is neither walked nor signalled.
    GroupRefused { pgid: Pid },
    /// A process a run left that a cancel or a close could not stop or
    /// close: what the system said.
    Unstopped { said: String },
    /// The registry naming a run's dead engine, which a cancel or a close
    /// could not remove: what the disk said.
    RegistryKept { said: String },
    /// A checkout of a run that could not go back to the project's pool, and
    /// stays the run's until `gc` collects it: the run, and what stopped it.
    CheckoutKept { run: RunId, said: String },
    /// Something a pass of collection could not do, left for a later pass:
    /// what it was about, and what stopped it.
    Uncollected(yunta_render::page::Uncollected),
    /// A pack's tree an install or a removal replaced, which stays on disk
    /// though the pack is in place: where it is, and what the disk said.
    ReplacedTreeKept { at: PathBuf, said: String },
    /// The keys of a run's manifest a newer binary wrote and this one does not
    /// read, said before anything is read of the run.
    UnreadManifestKeys { keys: Vec<String> },
}

/// Where an operation says its remarks, in the order it says them.
pub trait Remarks: Sync {
    fn remark(&self, remark: Remark);
}

/// What a run made to check something rather than to work says: all it
/// says but the estimate, which no such run, a probe's or a case's, is worth
/// a line of.
pub(crate) struct Unestimated<'a>(pub(crate) &'a dyn Remarks);

impl Remarks for Unestimated<'_> {
    fn remark(&self, remark: Remark) {
        if !matches!(remark, Remark::Estimate(_)) {
            self.0.remark(remark);
        }
    }
}

/// A closure takes each remark as it is said.
impl<F: Fn(Remark) + Sync> Remarks for F {
    fn remark(&self, remark: Remark) {
        self(remark)
    }
}
