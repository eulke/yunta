//! Reading a run's log the way a test asks about it.

use yunta_core::events::artifacts::ArtifactRef;
use yunta_core::events::{
    EventDraft, EventPayload, RunCreatedPayload, StoredEvent, TaskRegisteredPayload, TaskStatus,
    TaskStatusChangedPayload,
};
use yunta_core::{RunId, Seq, Task};
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

    /// Registers `task` the way a run does, and says what became of it.
    pub fn task(&self, task: &Task, status: TaskStatus) -> &Self {
        let registered = self.record(EventPayload::TaskRegistered(TaskRegisteredPayload {
            task_id: task.id.clone(),
            criteria: task.criteria.iter().map(Into::into).collect(),
            scope: task.scope.clone(),
            depends_on: task.depends_on.clone(),
        }));
        self.record(EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
            task_id: task.id.clone(),
            new_status: status,
            caused_by: registered,
        }));
        self
    }
}
