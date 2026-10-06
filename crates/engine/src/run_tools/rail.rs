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

use yunta_core::events::{EventPayload, SessionEvent, ToolTarget, WriteRefusedPayload};
use yunta_core::fence::{Fence, Verdict};
use yunta_core::rail::{unasked, RailAct, RailAnswer, RailCall, RailEvent};
use yunta_core::SessionId;

use super::session::SessionTools;

/// What one session's hooks are judged against, and what they were
/// refused.
///
/// The fence is set once the session's request is built, which is
/// before the session is dispatched and so before any hook can ask.
#[derive(Default)]
pub struct SessionRails {
    fence: OnceLock<Fence>,
    refused: Mutex<Vec<ToolTarget>>,
}

impl SessionRails {
    /// Holds the session to `fence`. A second fence is ignored: the one a
    /// session was opened with is the one it is judged against.
    pub fn fence(&self, fence: Fence) {
        self.fence.get_or_init(|| fence);
    }

    /// The fence the session was opened with, once it is set.
    pub(super) fn fence_of(&self) -> Option<&Fence> {
        self.fence.get()
    }

    /// Every write this session was refused, in the order it asked.
    pub fn refused(&self) -> Vec<ToolTarget> {
        self.refused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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
        let Some(fence) = self.rails.fence.get() else {
            return unasked(RailEvent::BeforeWrite, "this session has no fence yet");
        };
        let mut reasons = Vec::new();
        for path in paths {
            let Verdict::Refused(refusal) = fence.judge(&self.cwd, path) else {
                continue;
            };
            let target = shown(&refusal.target, &self.cwd);
            self.rails
                .refused
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(target.clone());
            let payload = EventPayload::Session(SessionEvent::WriteRefused(
                WriteRefusedPayload::new(session.clone(), target),
            ));
            if let Err(error) = self.append(payload).await {
                tracing::debug!(%error, "a refused write went unrecorded");
            }
            reasons.push(refusal.to_string());
        }
        match reasons.is_empty() {
            true => RailAnswer::Proceed,
            false => RailAnswer::Refuse {
                reason: reasons.join("\n\n"),
            },
        }
    }
}

/// A refused path as the log keeps it: relative to the work when it is
/// under it, because the log records what changed in the worktree, not
/// where the worktree sits on this host.
fn shown(target: &Path, cwd: &Path) -> ToolTarget {
    ToolTarget::of_path(target.strip_prefix(cwd).unwrap_or(target))
}
