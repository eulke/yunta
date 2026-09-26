//! `ToolsHost` — the world the per-run MCP listener serves.
//!
//! A run's tools answer about a run that exists on the log and whose
//! nodes never execute: what a test of them needs is a born run, the
//! directories `create_run` gives every run, and a listener opened for
//! one node. That is a layer of its own, below [`Bench`], which drives
//! whole runs and opens no listener.
//!
//! [`Bench`]: crate::Bench

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use yunta_core::events::{EventDraft, EventPayload, RunCreatedPayload, RunEvent, StoredEvent};
use yunta_core::{
    Clock, CommitSha, NodeId, RunId, Seq, SystemClock, Task, TaskId, TreeId, Workflow,
};
use yunta_engine::{
    open_session_listener, RunToolsHost, RunToolsSession, TaskAccess, Unit, UnitId,
};
use yunta_storage::Storage;

/// A born run, the directories it owns, and the host its tools answer
/// through.
pub struct ToolsHost {
    _root: TempDir,
    /// The run directory the host serves.
    pub run_dir: PathBuf,
    /// The event store every tool call appends to.
    pub storage: Storage,
    /// The run the host answers for.
    pub run_id: RunId,
    host: Arc<RunToolsHost>,
    clock: Arc<dyn Clock>,
}

impl ToolsHost {
    /// A host over `workflow_yaml`, stamping what it writes with the
    /// wall clock — for a test about what a tool call lands on the log
    /// rather than about when.
    pub fn over(workflow_yaml: &str) -> Self {
        Self::stamped_by(workflow_yaml, Arc::new(SystemClock))
    }

    /// The same host, stamping what it writes with `clock` — which is
    /// what proves an event carries the run's own reading of time.
    pub fn stamped_by(workflow_yaml: &str, clock: Arc<dyn Clock>) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let storage = Storage::open(&root.path().join("yunta.db")).expect("open storage");
        let run_id = RunId::from("run-tools-1");
        let workflow: Workflow = serde_norway::from_str(workflow_yaml).expect("parse workflow");
        let run_dir = born_run_dir(root.path());
        let host = Arc::new(RunToolsHost::new(
            &workflow,
            yunta_engine::HostOf {
                storage: storage.async_handle(),
                run_id: run_id.clone(),
                clock: clock.clone(),
                // A test here reads what a tool call lands on the log;
                // the mirror of it has its own test.
                observer: None,
                run_dir: run_dir.clone(),
                max_artifact_bytes: None,
                redactor: yunta_core::Redactor::default(),
                memo: Arc::new(yunta_engine::Memo::new(yunta_core::sha256_hex(
                    b"test-config",
                ))),
                process_registry: None,
                subprocess_vars: Vec::new(),
            },
        ));
        let hosted = ToolsHost {
            _root: root,
            run_dir,
            storage,
            run_id,
            host,
            clock,
        };
        // Every log opens with `run_created` — the listener's own
        // appends land on an already-born run in production too.
        hosted.record(
            None,
            EventPayload::Run(RunEvent::Created(RunCreatedPayload {
                manifest_hash: yunta_core::sha256_hex(b"test-manifest"),
                inputs: Default::default(),
                mode: Default::default(),
                promoted_from: None,
                yunta_schema: None,
                base_branch: "main".to_string(),
                base_commit: "deadbeef".into(),
            })),
        );
        hosted
    }

    /// Appends one event to the run's log, answering with the position
    /// storage gave it — what a run that reached this point would
    /// already have said.
    pub fn record(&self, node: Option<&str>, payload: EventPayload) -> Seq {
        self.storage
            .append(
                &EventDraft {
                    run_id: self.run_id.clone(),
                    node_id: node.map(NodeId::from),
                    payload,
                },
                self.clock.as_ref(),
            )
            .expect("append to the run's log")
    }

    /// Every event this run's log holds.
    pub fn events(&self) -> Vec<StoredEvent> {
        self.storage
            .events_for_run(&self.run_id)
            .expect("read events")
    }

    /// Where `node` writes what it declares, created as an attempt of
    /// that node would create it.
    pub fn staging(&self, node: &str) -> PathBuf {
        let dir = yunta_engine::run_dir::staging(&self.run_dir, &node.into());
        std::fs::create_dir_all(&dir).expect("create the node's staging directory");
        dir
    }

    /// The directory a session works in — where a request a tool writes
    /// for its caller lands.
    pub fn attempt_dir(&self) -> PathBuf {
        self._root.path().to_path_buf()
    }

    /// Opens the listener one node's session reaches its tools through —
    /// a task session's, when `task` names one: a task with nothing
    /// declared, worked in [`attempt_dir`](Self::attempt_dir).
    pub async fn session(&self, node: &str, task: Option<&str>) -> RunToolsSession {
        self.session_declaring(node, task, Vec::new()).await
    }

    /// The same listener, for a session that declares `declared` — what
    /// decides which artifacts its tools will take.
    pub async fn session_declaring(
        &self,
        node: &str,
        task: Option<&str>,
        declared: Vec<yunta_core::ArtifactSpec>,
    ) -> RunToolsSession {
        let task = task.map(|id| {
            self.task_access(
                Task {
                    id: TaskId::from(id),
                    title: String::new(),
                    scope: Vec::new(),
                    criteria: Vec::new(),
                    depends_on: Vec::new(),
                    notes: None,
                },
                Unit {
                    who: UnitId::Task(TaskId::from(id)),
                    worktree: self.attempt_dir(),
                    base: CommitSha::from_static("deadbeef"),
                    from: TreeId::from_static("deadbeef"),
                },
            )
        });
        self.open(node, task, declared).await
    }

    /// The listener a loop's session on `task` reaches its tools through.
    pub async fn task_session(&self, node: &str, task: TaskAccess) -> RunToolsSession {
        self.open(node, Some(task), Vec::new()).await
    }

    /// What a task session's tools reach for `task` worked in `unit`: the
    /// scope the task declared, nothing granted, and nothing staged.
    pub fn task_access(&self, task: Task, unit: Unit) -> TaskAccess {
        TaskAccess {
            scope: task.scope.clone(),
            task,
            unit,
            index: self.run_dir.join("check-index"),
            cancel: CancellationToken::new(),
            staged: Arc::new(OnceLock::from(Vec::new())),
        }
    }

    async fn open(
        &self,
        node: &str,
        task: Option<TaskAccess>,
        declared: Vec<yunta_core::ArtifactSpec>,
    ) -> RunToolsSession {
        let cwd = task
            .as_ref()
            .map_or_else(|| self.attempt_dir(), |task| task.unit.worktree.clone());
        open_session_listener(
            yunta_engine::RunToolsAccess {
                host: self.host.clone(),
                node: NodeId::from(node),
                // Run tools belong to a session, and a `prompt` node is
                // the one that opens one of its own.
                node_kind: yunta_core::NodeKind::Prompt {
                    prompt: yunta_core::PromptSource::Inline(String::new()),
                },
                declared,
            },
            task.map(Arc::new),
            cwd,
        )
        .await
        .expect("open the session's listener")
    }
}

/// A run directory under `root` holding what `create_run` gives every run:
/// the view the engine writes, and the working space every node stages in.
fn born_run_dir(root: &std::path::Path) -> PathBuf {
    let run_dir = root.join("run");
    for dir in [
        yunta_core::ARTIFACTS_DIR,
        yunta_engine::run_dir::SCRATCH_DIR,
    ] {
        std::fs::create_dir_all(run_dir.join(dir)).expect("create the run's own directories");
    }
    run_dir
}
