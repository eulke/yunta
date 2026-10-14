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
    EventPayload, Failure, ServiceReachablePayload, ServiceUnreachablePayload, SessionEvent,
    TokenUsage,
};
use yunta_core::port::{Adapter, AgentSession, SessionRequest};
use yunta_core::{AdapterError, Capability};
use yunta_storage::StorageError;

use super::session::{end_running, stream_session, SessionObserver};
use super::stream::{emit_audit, read_stream, Terms, Totals};
use super::DispatchOutcome;
use crate::run_tools::{Arrivals, SessionRails};

/// How [`dispatch_session`] failed: the adapter refused, a session it ended
/// could not be taken back, the engine could not stop a session it asked to
/// stop, or a session audit event could not be appended. The owners differ —
/// the adapter boundary, the session's own teardown, and the run's own
/// storage — so callers route each to its own node/run failure.
#[derive(Debug, Error)]
pub enum DispatchError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error("failed to append a session audit event")]
    Audit(#[source] StorageError),
    /// The session ended, and taking back what it set up failed.
    #[error("failed to take back what the session set up")]
    Release(#[source] AdapterError),
    /// The engine asked the session to stop and could not stop it. `release`
    /// is set when taking back what it set up failed as well, and `tokens` is
    /// what the session reported before the stop failed. The release error is
    /// boxed: it is the one large part of this variant, and `DispatchError`
    /// travels through every session's result.
    #[error("the engine could not stop the session: {detail}")]
    NotStopped {
        detail: String,
        release: Option<Box<AdapterError>>,
        tokens: TokenUsage,
    },
}

impl DispatchError {
    /// The same error, with `earlier` added to what a stopped session
    /// reported: a session picked back up after a lost service spent in
    /// both stretches.
    fn with_earlier_spend(self, earlier: TokenUsage) -> Self {
        match self {
            DispatchError::NotStopped {
                detail,
                release,
                tokens,
            } => DispatchError::NotStopped {
                detail,
                release,
                tokens: earlier + tokens,
            },
            other => other,
        }
    }
}

/// The failure a node records when its session could not be stopped: the
/// kill's error, and the release's when that failed as well. Re-exported next
/// to `DispatchError`.
pub(crate) fn not_stopped_failure(detail: &str, release: Option<&AdapterError>) -> Failure {
    let session_not_stopped = match release {
        Some(release) => {
            format!("{detail}; taking back what the session set up failed too: {release}")
        }
        None => detail.to_string(),
    };
    Failure::SessionNotStopped {
        session_not_stopped,
    }
}

/// What one session left behind: how it ended, what it spent, how much of
/// its writes its adapter's fence covered, and the sessions it opened — a
/// cache of one invocation of what the log already carries.
pub(crate) struct Dispatched {
    pub outcome: DispatchOutcome,
    pub tokens: TokenUsage,
    /// How much of the writes the session's fence covered, as its opening reported it.
    pub fence: Option<yunta_core::fence::Coverage>,
    /// The confinement the session that reported `fence` was opened under.
    pub confinement: Option<yunta_core::events::ConfinementRecord>,
    /// The sessions the stream opened, in order. The last is what the next
    /// attempt resumes when something it asked for changes.
    pub sessions: Vec<yunta_core::SessionId>,
}

/// How a session opens: the task it works, if any, the conversation it
/// continues, if any, the run tools whose calls say which of the session's
/// rejections reached a server, and the rail its CLI asks through.
#[derive(Clone, Copy, Default)]
pub(crate) struct Opening<'a> {
    pub(crate) task: Option<&'a yunta_core::TaskId>,
    pub(crate) resume: Option<Resume<'a>>,
    pub(crate) arrivals: Option<&'a Arrivals>,
    /// The rail the session's CLI asks through. The session closes with a
    /// summary of the calls the rail answered for it.
    pub(crate) rails: Option<&'a SessionRails>,
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
            dispatched.sessions.last().cloned(),
            remaining(&request, &dispatched.tokens, worked),
        ) else {
            break;
        };
        let DispatchOutcome::Failed { error, .. } = &dispatched.outcome else {
            break;
        };
        let lost = ServiceUnreachablePayload {
            session_id: session.clone(),
            message: yunta_core::describe(error),
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
        let opening = resumed(opening, &session, &request);
        working = Instant::now() - worked;
        let (next, again_cut) = stream_session(adapter, again, cancel, audit, opening)
            .await
            .map_err(|error| error.with_earlier_spend(dispatched.tokens))?;
        dispatched = joined(dispatched, next);
        cut_off = again_cut;
    }
    Ok(dispatched)
}

/// How a session picked back up after a lost service is opened: on the
/// conversation it continues, with the request's brief for a fresh one.
fn resumed<'a>(
    opening: Opening<'a>,
    session: &'a yunta_core::SessionId,
    request: &'a SessionRequest,
) -> Opening<'a> {
    Opening {
        task: opening.task,
        resume: Some(Resume {
            session,
            fresh_prompt: Some(&request.prompt),
        }),
        arrivals: opening.arrivals,
        rails: opening.rails,
    }
}

/// Reads a session to its terminal outcome, and ends it when the read was cut
/// short by a cancel or a budget. A session the engine cannot stop fails with
/// what it reported, which `totals` holds.
pub(super) async fn read_and_end(
    session: &mut dyn AgentSession,
    cancel: &CancellationToken,
    terms: &Terms<'_>,
    opening: Opening<'_>,
    continues: Option<&yunta_core::SessionId>,
    totals: &mut Totals,
) -> Result<(Option<DispatchOutcome>, bool), DispatchError> {
    let (terminal, cancelled) =
        read_stream(session, cancel, terms, opening, continues, totals).await?;
    if cancelled || matches!(terminal, Some(DispatchOutcome::BudgetExceeded { .. })) {
        end_running(session)
            .await
            .map_err(|source| DispatchError::NotStopped {
                detail: source.to_string(),
                release: None,
                tokens: totals.tokens,
            })?;
    }
    Ok((terminal, cancelled))
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
/// up: the last one's ending, everything any of them spent, and every
/// session any of them opened.
fn joined(before: Dispatched, after: Dispatched) -> Dispatched {
    let mut tokens = before.tokens;
    tokens += after.tokens;
    let mut sessions = before.sessions;
    sessions.extend(after.sessions);
    Dispatched {
        outcome: after.outcome,
        tokens,
        fence: after.fence.or(before.fence),
        confinement: after.confinement.or(before.confinement),
        sessions,
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
        let shell = audit
            .and_then(|(observer, _)| observer.shell_service())
            .cloned()
            .unwrap_or_default();
        let back = self.wait(adapter, request, &shell, cancel, awake).await;
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
        shell: &std::collections::BTreeMap<String, String>,
        cancel: &CancellationToken,
        awake: impl Fn() -> Instant,
    ) -> Back {
        let from = awake();
        // Long enough to ride out a network that drops for a while — a
        // train, a router restarting — and short of leaving a run hanging
        // on a service that is gone.
        let bound = Duration::from_secs(30 * 60);
        loop {
            let answers = adapter.reachable(request, shell).await;
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
