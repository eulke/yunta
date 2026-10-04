//! One session's dispatch, from opening to its end — through the service
//! behind it going away and coming back.
//!
//! A session cut off from its service lost nothing of its work: the
//! conversation is the adapter's to pick back up, and the tree is as the
//! session left it. So the engine waits for the service to answer — never
//! past a cancellation, and never longer than a bound of the time the host
//! is awake — and the same session goes on, with what is left of its
//! budget. Only an adapter that cannot resume a session, or a service that
//! stays away, fails it the way any failure fails.

use std::time::{Duration, Instant};

use thiserror::Error;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{
    EventPayload, ServiceReachablePayload, ServiceUnreachablePayload, SessionEvent, TokenUsage,
};
use yunta_core::port::{Adapter, SessionRequest};
use yunta_core::{AdapterError, Capability};
use yunta_storage::StorageError;

use super::session::{stream_session, SessionObserver};
use super::stream::emit_audit;
use super::DispatchOutcome;

/// How [`dispatch_session`] failed: the adapter refused, or a session
/// audit event could not be appended. The two have different owners —
/// the adapter boundary versus the run's own storage — so callers route
/// each to its own node/run failure.
#[derive(Debug, Error)]
pub enum DispatchError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error("failed to append a session audit event")]
    Audit(#[source] StorageError),
}

/// What one session left behind: how it ended, what it spent, and how
/// much of its writes its adapter's fence covered — a cache of one
/// invocation of what the log already carries.
pub(crate) struct Dispatched {
    pub outcome: DispatchOutcome,
    pub tokens: TokenUsage,
    pub fence: Option<yunta_core::fence::Coverage>,
    /// The session the stream opened, once it did: what the next
    /// attempt resumes when something it asked for changes.
    pub session: Option<yunta_core::SessionId>,
    /// Every write the fence refused it.
    pub refused: Vec<yunta_core::events::ToolTarget>,
}

/// How a session opens: the task it works, if any, and the conversation
/// it continues, if any.
#[derive(Clone, Copy, Default)]
pub(crate) struct Opening<'a> {
    pub(crate) task: Option<&'a yunta_core::TaskId>,
    pub(crate) resume: Option<Resume<'a>>,
}

/// A conversation to pick back up, and what a fresh session is told
/// instead when the adapter cannot pick it up — `None` when there is no
/// such way back and not resuming is the caller's failure to report.
#[derive(Clone, Copy)]
pub(crate) struct Resume<'a> {
    pub(crate) session: &'a yunta_core::SessionId,
    pub(crate) fresh_prompt: Option<&'a str>,
}

/// What a session picked back up after its service came back is told.
const CONTINUE: &str = "The connection to your model's service dropped and is back. Nothing \
                        you did was lost: continue exactly where you left off.";

/// Opens a session for `request` and drives it to its end. Shared by the
/// task cycle and by prompt-node execution: the request differs, the
/// enforcement does not.
pub(crate) async fn dispatch_session(
    adapter: &dyn Adapter,
    request: SessionRequest,
    cancel: &CancellationToken,
    audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    opening: Opening<'_>,
) -> Result<Dispatched, DispatchError> {
    let mut working = Instant::now();
    let (mut dispatched, mut cut_off) =
        stream_session(adapter, request.clone(), cancel, audit, opening).await?;
    let mut away = Away::default();
    let resumable = adapter.capabilities().declares(Capability::ResumeSession);
    while cut_off && resumable {
        let worked = working.elapsed();
        let (Some(session), Some(again)) = (
            dispatched.session.clone(),
            remaining(&request, &dispatched.tokens, worked),
        ) else {
            break;
        };
        let DispatchOutcome::Failed { message, .. } = &dispatched.outcome else {
            break;
        };
        let lost = ServiceUnreachablePayload {
            session_id: session.clone(),
            message: message.clone(),
        };
        match away
            .reconnect(lost, adapter, &request, cancel, audit)
            .await?
        {
            Back::Answered => {}
            Back::Cancelled => {
                dispatched.outcome = DispatchOutcome::Cancelled;
                break;
            }
            Back::Never => break,
        }
        let opening = Opening {
            task: opening.task,
            resume: Some(Resume {
                session: &session,
                fresh_prompt: Some(&request.prompt),
            }),
        };
        working = Instant::now() - worked;
        let (next, again_cut) = stream_session(adapter, again, cancel, audit, opening).await?;
        dispatched = joined(dispatched, next);
        cut_off = again_cut;
    }
    Ok(dispatched)
}

/// `request` with the budget a session that already spent `tokens` over
/// `worked` has left, telling it to go on — `None` when nothing is left.
fn remaining(
    request: &SessionRequest,
    tokens: &TokenUsage,
    worked: Duration,
) -> Option<SessionRequest> {
    let mut budget = request.budget;
    if let Some(max) = budget.max_tokens {
        budget.max_tokens = Some(max.checked_sub(tokens.total()).filter(|left| *left > 0)?);
    }
    if let Some(timeout) = budget.timeout {
        budget.timeout = Some(timeout.checked_sub(worked).filter(|left| !left.is_zero())?);
    }
    let told = crate::run::session_plan::with_prompt(request.clone(), CONTINUE.to_string());
    Some(crate::run::session_plan::with_budget(told, budget))
}

/// One session's dispatch, as the stretches it was picked back up in add
/// up: the last one's ending, everything any of them spent and refused.
fn joined(before: Dispatched, after: Dispatched) -> Dispatched {
    let mut tokens = before.tokens;
    tokens += after.tokens;
    let mut refused = before.refused;
    refused.extend(after.refused);
    Dispatched {
        outcome: after.outcome,
        tokens,
        fence: after.fence.or(before.fence),
        session: after.session.or(before.session),
        refused,
    }
}

/// How a wait for the service ended.
enum Back {
    Answered,
    Cancelled,
    /// It did not answer within the bound.
    Never,
}

/// What one dispatch spent waiting for its service, across every time it
/// lost it, and how long it waits before it looks again.
struct Away {
    waited: Duration,
    pause: Duration,
}

impl Default for Away {
    fn default() -> Self {
        Away {
            waited: Duration::ZERO,
            pause: Duration::from_secs(1),
        }
    }
}

impl Away {
    /// Says the session lost its service, waits for the service, and says
    /// when it answers again.
    async fn reconnect(
        &mut self,
        lost: ServiceUnreachablePayload,
        adapter: &dyn Adapter,
        request: &SessionRequest,
        cancel: &CancellationToken,
        audit: Option<(&dyn SessionObserver, &yunta_core::NodeId)>,
    ) -> Result<Back, DispatchError> {
        let said = |payload| emit_audit(audit, EventPayload::Session(payload));
        said(SessionEvent::ServiceUnreachable(lost))
            .await
            .map_err(DispatchError::Audit)?;
        let awake = || audit.map_or_else(Instant::now, |(observer, _)| observer.awake());
        let from = awake();
        let back = self.wait(adapter, request, cancel, awake).await;
        let waited = awake().saturating_duration_since(from);
        self.waited += waited;
        if !matches!(back, Back::Answered) {
            return Ok(back);
        }
        if let Some((observer, _)) = audit {
            if !observer
                .host_settled(cancel)
                .await
                .map_err(DispatchError::Audit)?
            {
                return Ok(Back::Cancelled);
            }
        }
        let waited_ms = u64::try_from(waited.as_millis()).unwrap_or(u64::MAX);
        said(SessionEvent::ServiceReachable(ServiceReachablePayload {
            waited_ms,
        }))
        .await
        .map_err(DispatchError::Audit)?;
        Ok(Back::Answered)
    }

    /// Looks for the service with growing pauses until it answers. An
    /// adapter that cannot tell where its service is gets one pause, and
    /// the session itself is the next look.
    async fn wait(
        &mut self,
        adapter: &dyn Adapter,
        request: &SessionRequest,
        cancel: &CancellationToken,
        awake: impl Fn() -> Instant,
    ) -> Back {
        let from = awake();
        // Long enough to ride out a network that drops for a while — a
        // train, a router restarting — and short of leaving a run hanging
        // on a service that is gone.
        let bound = Duration::from_secs(30 * 60);
        loop {
            let answers = adapter.reachable(request).await;
            if answers == Some(true) {
                return Back::Answered;
            }
            if self.waited + awake().saturating_duration_since(from) >= bound {
                return Back::Never;
            }
            if tokio::time::timeout(self.pause, cancel.cancelled())
                .await
                .is_ok()
            {
                return Back::Cancelled;
            }
            self.pause = (self.pause * 2).min(Duration::from_secs(60));
            if answers.is_none() {
                return Back::Answered;
            }
        }
    }
}
