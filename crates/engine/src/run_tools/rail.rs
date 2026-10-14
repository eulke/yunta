//! What the engine answers a session's hooks.
//!
//! A CLI's hook asks before a write; the answer comes from the session's
//! own fence, judged here, and every refusal is on the log before the
//! hook hears it — so a refused write is recorded the same way whichever
//! CLI made it, and whatever that CLI does or does not print about it.
//! After a write, what it wrote is formatted (`format`); before the
//! session stops, its work is judged (`stop`).
//!
//! A call is answered under the session that owns the listener it reached,
//! never under a session its body names, and its answer is timed for the
//! session's summary. A failure of the rail is recorded under that session,
//! or kept as unrecorded when the log does not take it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::watch;
use yunta_core::events::{
    EventPayload, RailFailedPayload, SessionEvent, StoredEvent, ToolTarget, WriteRefusedPayload,
};
use yunta_core::fence::{Fence, Refusal, Verdict};
use yunta_core::rail::answer::{FormatAnswer, HookAnswer, StopAnswer, WriteAnswer, WriteRefusal};
use yunta_core::rail::hook::{RailFailureCause, RailReport};
use yunta_core::rail::timing::{Histogram, RailSummaryPayload};
use yunta_core::rail::{RailAct, RailCall, RailEvent, ROUTE};
use yunta_core::{Confinement, ModelName, SessionId, Unredactable};
use yunta_storage::StorageError;

use super::session::{RunToolError, SessionTools};

/// What one session's hooks are judged against, the session they are
/// answered under, and the refusal the log could not keep, if one was.
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
    /// The session the stream opened last on this listener: a resumed
    /// session replaces the one it continues.
    owner: watch::Sender<Option<SessionId>>,
    /// The first model a hook call named, which the session's summary reports.
    model: OnceLock<ModelName>,
    /// How long each moment's answers took, kept for the session's summary
    /// and never written to the log.
    timings: Mutex<BTreeMap<RailEvent, Histogram>>,
}

impl SessionRails {
    /// Holds the session to `fence`, judged beside `confinement`. A second
    /// pair is ignored: the one a session was opened with is the one it is
    /// judged against.
    pub fn fence(&self, fence: Fence, confinement: Confinement) {
        self.fence.get_or_init(|| fence);
        self.confinement.get_or_init(|| confinement);
    }

    /// Names the session this listener serves. The stream calls it at each opening.
    pub fn own(&self, session: SessionId) {
        self.owner.send_replace(Some(session));
    }

    /// The fence and the confinement the session was opened with, once set.
    pub(super) fn fence_of(&self) -> Option<(&Fence, &Confinement)> {
        Some((self.fence.get()?, self.confinement.get()?))
    }

    /// The session this listener serves, waiting until the stream opens one.
    /// `None` only when the rail itself is gone.
    pub(super) async fn owner(&self) -> Option<SessionId> {
        let mut owner = self.owner.subscribe();
        let opened = owner.wait_for(Option::is_some).await.ok()?;
        (*opened).clone()
    }

    /// The session this listener serves, when the stream has opened one.
    pub(super) fn current_owner(&self) -> Option<SessionId> {
        self.owner.borrow().clone()
    }

    /// Keeps the first model a hook call named.
    pub(super) fn hear_model(&self, model: Option<&ModelName>) {
        if let Some(model) = model {
            self.model.get_or_init(|| model.clone());
        }
    }

    /// Adds the time one moment's answer took to that moment's histogram.
    pub(super) fn observe(&self, event: RailEvent, took: Duration) {
        self.timings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(event)
            .or_default()
            .observe(took);
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
    pub(crate) fn left_unrecorded(&self, error: RunToolError) {
        *self.unrecorded.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
    }

    /// The summary of this session's rail calls: the session they were
    /// answered under, the first model they named, and how long each moment
    /// took. `None` when no session has opened on this listener.
    pub(crate) fn take_summary(&self) -> Option<RailSummaryPayload> {
        let session_id = self.current_owner()?;
        let moments = std::mem::take(&mut *self.timings.lock().unwrap_or_else(|e| e.into_inner()));
        Some(RailSummaryPayload {
            session_id,
            model: self.model.get().cloned(),
            moments,
        })
    }
}

impl SessionTools {
    /// Whether the session may write every one of `paths`. Each refused path
    /// is recorded before the session hears of it, and the reason it reads
    /// joins the fence's own text for each refusal.
    pub(super) async fn before_write(&self, session: &SessionId, paths: &[PathBuf]) -> WriteAnswer {
        let Some((fence, confinement)) = self.rails.fence_of() else {
            self.failed(
                Some(session),
                Some(RailEvent::BeforeWrite),
                RailFailureCause::NoFence,
            )
            .await;
            return WriteAnswer::unasked("this session has no fence yet");
        };
        let mut refusals: Vec<Refusal> = Vec::new();
        for path in paths {
            let Verdict::Refused(refusal) = fence.judge(&self.cwd, path, confinement) else {
                continue;
            };
            let target = shown(&refusal.target, &self.cwd);
            let payload = EventPayload::Session(SessionEvent::WriteRefused(
                WriteRefusedPayload::new(session.clone(), target),
            ));
            if let Err(error) = self.append(payload).await {
                self.rails.left_unrecorded(error);
            }
            refusals.push(refusal);
        }
        match WriteRefusal::of(&refusals, self.naming) {
            Some(reason) => WriteAnswer::Refuse { reason },
            None => WriteAnswer::Proceed,
        }
    }

    /// Records a failure of the rail under `session`, when one is known, at
    /// the moment it happened. A log that does not take the record keeps the
    /// error as unrecorded.
    pub(super) async fn failed(
        &self,
        session: Option<&SessionId>,
        event: Option<RailEvent>,
        cause: RailFailureCause,
    ) {
        let payload = RailFailedPayload {
            session_id: session.cloned(),
            event,
            cause,
        };
        let payload = EventPayload::Session(SessionEvent::RailFailed(payload));
        if let Err(error) = self.append(payload).await {
            self.rails.left_unrecorded(error);
        }
    }
}

/// The rail's routes on a session's listener: one per moment, and the
/// failure report. The routes share one record of a call that does not
/// read, so each cause is recorded once per listener.
pub(super) fn rail_routes(tools: SessionTools) -> axum::Router {
    let undecodable = Arc::new(AtomicBool::new(false));
    let mut router = axum::Router::new();
    for event in RailEvent::ALL {
        let (tools, undecodable) = (tools.clone(), Arc::clone(&undecodable));
        let moment = axum::routing::post(move |body: axum::body::Bytes| {
            let (tools, undecodable) = (tools.clone(), Arc::clone(&undecodable));
            async move { answer_moment(&tools, &undecodable, event, &body).await }
        });
        router = router.route(&format!("{}/{}", ROUTE, event.as_str()), moment);
    }
    let reporting = axum::routing::post(move |body: axum::body::Bytes| {
        let (tools, undecodable) = (tools.clone(), Arc::clone(&undecodable));
        async move { report_failure(&tools, &undecodable, &body).await }
    });
    router.route(&format!("{}/report", ROUTE), reporting)
}

/// The answer to a call that reached `moment`'s route, given under the
/// session the listener serves. A call that does not read as `moment` is
/// answered `400`.
async fn answer_moment(
    tools: &SessionTools,
    undecodable: &AtomicBool,
    moment: RailEvent,
    body: &[u8],
) -> Response {
    let read = serde_json::from_slice::<RailCall>(body)
        .ok()
        .filter(|call| call.act.moment() == moment);
    let Some(call) = read else {
        return unreadable(tools, undecodable, Some(moment)).await;
    };
    tools.rails.hear_model(call.model.as_ref());
    let Some(session) = tools.rails.owner().await else {
        return unasked_response(&call.act);
    };
    let started = Instant::now();
    let response = match call.act {
        RailAct::BeforeWrite { paths } => {
            json_response(&tools.before_write(&session, &paths).await)
        }
        RailAct::AfterWrite { paths } => json_response(&tools.after_write(&session, &paths).await),
        RailAct::BeforeStop => json_response(&tools.before_stop(&session).await),
    };
    tools.rails.observe(moment, started.elapsed());
    response
}

/// `400` for a call that does not read as its route's moment. The first
/// such call is recorded under the session the listener serves; the rest
/// only get the `400`.
async fn unreadable(
    tools: &SessionTools,
    undecodable: &AtomicBool,
    event: Option<RailEvent>,
) -> Response {
    if !undecodable.swap(true, Ordering::Relaxed) {
        let session = tools.rails.current_owner();
        tools
            .failed(session.as_ref(), event, RailFailureCause::Decode)
            .await;
    }
    StatusCode::BAD_REQUEST.into_response()
}

/// Records the failure a hook reports, under the session the listener
/// serves. The body names no session, and none of it is trusted for one.
async fn report_failure(tools: &SessionTools, undecodable: &AtomicBool, body: &[u8]) -> Response {
    let Ok(report) = serde_json::from_slice::<RailReport>(body) else {
        return unreadable(tools, undecodable, None).await;
    };
    let session = tools.rails.owner().await;
    tools
        .failed(session.as_ref(), Some(report.event), report.cause)
        .await;
    StatusCode::OK.into_response()
}

/// The answer to a call when the listener serves no session: the answer its
/// moment gives when nothing can be asked.
fn unasked_response(act: &RailAct) -> Response {
    const WHY: &str = "this listener serves no session";
    match act {
        RailAct::BeforeWrite { .. } => json_response(&WriteAnswer::unasked(WHY)),
        RailAct::AfterWrite { .. } => json_response(&FormatAnswer::unasked(WHY)),
        RailAct::BeforeStop => json_response(&StopAnswer::unasked(WHY)),
    }
}

/// `answer` as the JSON body a hook reads.
fn json_response<A: serde::Serialize>(answer: &A) -> Response {
    match serde_json::to_string(answer) {
        Ok(body) => (
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
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

/// Takes the refusal `rails` could not keep, and names its storage cause:
/// a refusal the engine answered and could not write fails the attempt or
/// the node that asked, with the cause the log gave. A log append fails with
/// one of the two causes here; any other error is not one the rail stores,
/// so it gives no cause to fail with.
pub(crate) fn unrecorded_storage_cause(rails: &SessionRails) -> Option<StorageError> {
    match rails.take_unrecorded()? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::RunId;

    #[test]
    fn a_refusal_the_log_could_not_keep_is_taken_once_with_its_cause() {
        let rails = SessionRails::default();
        rails.left_unrecorded(RunToolError::Storage {
            source: StorageError::Append {
                run_id: RunId::from("run-test"),
                source: "audit storage is down".into(),
            },
        });
        assert!(unrecorded_storage_cause(&rails).is_some());
        assert!(unrecorded_storage_cause(&rails).is_none());
    }
}
