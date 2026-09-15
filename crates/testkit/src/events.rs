//! Reading a run's log the way a test asks about it, and writing the
//! events a run would have written.

use std::sync::Arc;

use yunta_core::events::artifacts::ArtifactRef;
use yunta_core::events::{ArtifactEvent, RunEvent, TaskEvent};
use yunta_core::events::{
    EventDraft, EventPayload, RunCreatedPayload, StoredEvent, TaskRegisteredPayload, TaskStatus,
    TaskStatusChangedPayload,
};
use yunta_core::{Clock, CommitSha, RunId, Seq, Task, TaskId};
use yunta_storage::Storage;

/// Every `artifact_accepted` on `events`, in log order — one entry per
/// event, never folded, so a test sees an identity accepted twice as two
/// acceptances and can say where each sits in the log.
pub fn accepted(events: &[StoredEvent]) -> Vec<ArtifactRef> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Artifacts(ArtifactEvent::Accepted(p))) => Some(ArtifactRef {
                producer: event.node_id.clone(),
                artifact: p.artifact.clone(),
                content_hash: p.content_hash.clone(),
                origin: p.origin.clone(),
                seq: event.seq,
            }),
            _ => None,
        })
        .collect()
}

/// The `task_registered` a run writes for `task` — what the run has to
/// do about it, in the task's own terms.
pub fn task_registered(task: &Task) -> EventPayload {
    EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
        task_id: task.id.clone(),
        criteria: task.criteria.iter().map(Into::into).collect(),
        scope: task.scope.clone(),
        depends_on: task.depends_on.clone(),
    }))
}

/// The `task_status_changed` a run writes about `task`: the status it
/// reached, the commit its work landed at — which a `done` names and no
/// other status does — and the event that justifies the transition.
///
/// The status decides, not the commit: a `done` carries the one it is
/// given, and every other status carries none, exactly as the payload's
/// own constructors allow. A test about a log that carries one anyway —
/// written by something that is not this engine — builds that event with
/// [`status_changed_carrying`].
pub fn task_status_changed(
    task: &TaskId,
    status: TaskStatus,
    commit: Option<&CommitSha>,
    caused_by: Seq,
) -> EventPayload {
    let changed = match (status, commit) {
        (TaskStatus::Done, Some(commit)) => {
            TaskStatusChangedPayload::done(task.clone(), caused_by, commit.clone())
        }
        (status, _) => TaskStatusChangedPayload::to(task.clone(), status, caused_by),
    };
    EventPayload::Tasks(TaskEvent::StatusChanged(changed))
}

/// A `task_status_changed` carrying a commit under a status that has no
/// business naming one — the shape this engine's constructors refuse and
/// a persisted log may still hold, because a log is read from whatever
/// wrote it.
///
/// Built through the wire form, which is how such an event actually
/// reaches a reader: off disk, never out of a constructor.
pub fn status_changed_carrying(
    task: &TaskId,
    status: TaskStatus,
    commit: &CommitSha,
    caused_by: Seq,
) -> EventPayload {
    let wire = serde_json::json!({
        "kind": "task_status_changed",
        "task_id": task.as_str(),
        "new_status": serde_json::to_value(status).expect("a status is a string on the wire"),
        "caused_by": caused_by.get(),
        "commit": commit.as_str(),
    });
    serde_json::from_value(wire).expect("the wire shape of a task_status_changed")
}

/// Another run's log, written by hand: what a run that hands something
/// over already said about it.
///
/// A run's first event is always its `run_created` — the hash chain's
/// genesis — so opening one writes that before anything a test states.
///
/// The clock is the one the surrounding test runs on, so every event
/// this log holds is stamped by the same reading of time the run under
/// test is stamped by — a planted log dated from the wall clock is one
/// no assertion about order can trust.
pub struct SourceLog<'a> {
    storage: &'a Storage,
    run_id: RunId,
    clock: Arc<dyn Clock>,
}

impl<'a> SourceLog<'a> {
    /// Opens `run_id`'s log on `storage`, born, stamped by `clock`.
    pub fn open(storage: &'a Storage, run_id: &RunId, clock: Arc<dyn Clock>) -> Self {
        let log = SourceLog {
            storage,
            run_id: run_id.clone(),
            clock,
        };
        log.record(EventPayload::Run(RunEvent::Created(RunCreatedPayload {
            manifest_hash: yunta_core::sha256_hex(run_id.as_str().as_bytes()),
            inputs: Default::default(),
            mode: Default::default(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: yunta_core::sha256_hex(b"base").as_str().into(),
        })));
        log
    }

    /// Appends one run-level event, answering with the position storage
    /// gave it.
    pub fn record(&self, payload: EventPayload) -> Seq {
        self.storage
            .append(
                &EventDraft {
                    run_id: self.run_id.clone(),
                    node_id: None,
                    payload,
                },
                self.clock.as_ref(),
            )
            .expect("append to the source log")
    }

    /// Registers `task` the way a run does, and says what became of it —
    /// `commit` is where the work landed, which only a `done` names.
    pub fn task(&self, task: &Task, status: TaskStatus, commit: Option<&CommitSha>) -> &Self {
        let registered = self.record(task_registered(task));
        self.record(task_status_changed(&task.id, status, commit, registered));
        self
    }
}
