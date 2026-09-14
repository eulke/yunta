//! Reading a run's log the way a test asks about it, and writing the
//! events a run would have written.

use yunta_core::events::artifacts::ArtifactRef;
use yunta_core::events::{
    EventBody, EventDraft, EventPayload, RunCreatedPayload, StoredEvent, TaskRegisteredPayload,
    TaskStatus, TaskStatusChangedPayload,
};
use yunta_core::{CommitSha, RunId, Seq, Task, TaskId};
use yunta_storage::Storage;

/// Every `artifact_accepted` on `events`, in log order — one entry per
/// event, never folded, so a test sees an identity accepted twice as two
/// acceptances and can say where each sits in the log.
pub fn accepted(events: &[StoredEvent]) -> Vec<ArtifactRef> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::ArtifactAccepted(p)) => Some(ArtifactRef {
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
    EventPayload::TaskRegistered(TaskRegisteredPayload {
        task_id: task.id.clone(),
        criteria: task.criteria.iter().map(Into::into).collect(),
        scope: task.scope.clone(),
        depends_on: task.depends_on.clone(),
    })
}

/// The `task_status_changed` a run writes about `task`: the status it
/// reached, the commit its work landed at — which a `done` names and no
/// other status does — and the event that justifies the transition.
pub fn task_status_changed(
    task: &TaskId,
    status: TaskStatus,
    commit: Option<&CommitSha>,
    caused_by: Seq,
) -> EventPayload {
    EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
        task_id: task.clone(),
        new_status: status,
        caused_by,
        commit: commit.cloned(),
    })
}

/// One event as a log holds it: position `seq` of `run`, with no node
/// behind it — what a test hands a function that reads a log without
/// storing one.
pub fn stored(run: &RunId, seq: u64, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        run_id: run.clone(),
        seq: seq.into(),
        timestamp: chrono::DateTime::UNIX_EPOCH,
        node_id: None,
        body: EventBody::Known(payload),
    }
}

/// The same, attributed to the node that wrote it: what a reading keyed
/// by node answers from.
pub fn stored_for(run: &RunId, seq: u64, node: &str, payload: EventPayload) -> StoredEvent {
    StoredEvent {
        node_id: Some(node.into()),
        ..stored(run, seq, payload)
    }
}

/// Another run's log, written by hand: what a run that hands something
/// over already said about it.
///
/// A run's first event is always its `run_created` — the hash chain's
/// genesis — so opening one writes that before anything a test states.
pub struct SourceLog<'a> {
    storage: &'a Storage,
    run_id: RunId,
}

impl<'a> SourceLog<'a> {
    /// Opens `run_id`'s log on `storage`, born.
    pub fn open(storage: &'a Storage, run_id: &RunId) -> Self {
        let log = SourceLog {
            storage,
            run_id: run_id.clone(),
        };
        log.record(EventPayload::RunCreated(RunCreatedPayload {
            manifest_hash: yunta_core::sha256_hex(run_id.as_str().as_bytes()),
            inputs: Default::default(),
            mode: Default::default(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: yunta_core::sha256_hex(b"base").as_str().into(),
        }));
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
                &yunta_core::SystemClock,
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
