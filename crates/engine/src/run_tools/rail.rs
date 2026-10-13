//! What the engine answers a session's hooks.
//!
//! A CLI's hook asks before a write; the answer comes from the session's
//! own fence, judged here, and every refusal is on the log before the
//! hook hears it — so a refused write is recorded the same way whichever
//! CLI made it, and whatever that CLI does or does not print about it.
//! After a write, what it wrote is formatted (`format`); before the
//! session stops, its work is judged (`stop`).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use yunta_core::events::{
    EventPayload, SessionEvent, StoredEvent, ToolTarget, WriteRefusedPayload,
};
use yunta_core::fence::{Fence, Verdict};
use yunta_core::rail::{unasked, RailAct, RailAnswer, RailCall, RailEvent};
use yunta_core::{Confinement, SessionId, Unredactable};
use yunta_storage::StorageError;

use super::session::{RunToolError, SessionTools};

/// What one session's hooks are judged against, and the refusal the log
/// could not keep, if one was.
///
/// The fence and the confinement are set once the session's request is
/// built, which is before the session is dispatched and so before any hook
/// can ask.
#[derive(Default)]
pub struct SessionRails {
    fence: OnceLock<Fence>,
    /// What the session may write and reach, which its fence is judged beside.
    confinement: OnceLock<Confinement>,
    /// The most recent refusal whose log append failed. The engine answered
    /// it all the same; the attempt that dispatched the session reads it
    /// and fails with the storage cause.
    unrecorded: Mutex<Option<RunToolError>>,
}

impl SessionRails {
    /// Holds the session to `fence`, judged beside `confinement`. A second
    /// pair is ignored: the one a session was opened with is the one it is
    /// judged against.
    pub fn fence(&self, fence: Fence, confinement: Confinement) {
        self.fence.get_or_init(|| fence);
        self.confinement.get_or_init(|| confinement);
    }

    /// The fence and the confinement the session was opened with, once set.
    pub(super) fn fence_of(&self) -> Option<(&Fence, &Confinement)> {
        Some((self.fence.get()?, self.confinement.get()?))
    }

    /// The refusal whose log append failed, taken: read once the session
    /// has ended.
    pub(crate) fn take_unrecorded(&self) -> Option<RunToolError> {
        self.unrecorded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Keeps `error` as the refusal whose log append failed.
    fn keep_unrecorded(&self, error: RunToolError) {
        *self.unrecorded.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
    }
}

impl SessionTools {
    /// The answer to one hook call.
    pub(super) async fn rail(&self, call: RailCall) -> RailAnswer {
        match call.act {
            RailAct::BeforeWrite { paths } => self.before_write(&call.session_id, &paths).await,
            RailAct::AfterWrite { paths } => self.after_write(&call.session_id, &paths).await,
            RailAct::BeforeStop => self.before_stop(&call.session_id).await,
        }
    }

    /// Whether the session may write every one of `paths`. Each refused
    /// path is recorded before the session hears of it; the refusal it
    /// reads is the fence's own text, one per path.
    async fn before_write(&self, session: &SessionId, paths: &[PathBuf]) -> RailAnswer {
        let Some((fence, confinement)) = self.rails.fence_of() else {
            return unasked(RailEvent::BeforeWrite, "this session has no fence yet");
        };
        let mut reasons = Vec::new();
        for path in paths {
            let Verdict::Refused(refusal) = fence.judge(&self.cwd, path, confinement) else {
                continue;
            };
            let target = shown(&refusal.target, &self.cwd);
            let payload = EventPayload::Session(SessionEvent::WriteRefused(
                WriteRefusedPayload::new(session.clone(), target),
            ));
            if let Err(error) = self.append(payload).await {
                self.rails.keep_unrecorded(error);
            }
            reasons.push(refusal.message(self.naming));
        }
        match reasons.is_empty() {
            true => RailAnswer::Proceed,
            false => RailAnswer::Refuse {
                reason: reasons.join("\n\n"),
            },
        }
    }
}

/// Every write these sessions were refused, in the order the log holds
/// them. Refusals are read from the log only, so this is the one place
/// they are derived.
pub fn refused_writes(events: &[StoredEvent], sessions: &[SessionId]) -> Vec<ToolTarget> {
    events
        .iter()
        .filter_map(|stored| match stored.payload() {
            Some(EventPayload::Session(SessionEvent::WriteRefused(refused)))
                if sessions.contains(&refused.session_id) =>
            {
                Some(refused.target.clone())
            }
            _ => None,
        })
        .collect()
}

/// The storage cause of a refusal the engine answered and could not write.
/// A log append fails with one of the two causes here; any other error is
/// not one the rail stores, so it gives an attempt no cause to fail with.
pub(crate) fn unrecorded_cause(error: RunToolError) -> Option<StorageError> {
    match error {
        RunToolError::Storage { source } => Some(source),
        RunToolError::Unredactable { kind } => {
            Some(StorageError::Unredactable(Unredactable { kind }))
        }
        _ => None,
    }
}

/// A refused path as the log keeps it: relative to the work when it is
/// under it, because the log records what changed in the worktree, not
/// where the worktree sits on this host.
fn shown(target: &Path, cwd: &Path) -> ToolTarget {
    ToolTarget::of_path(target.strip_prefix(cwd).unwrap_or(target))
}
