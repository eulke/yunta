//! Whether a session may end its turn: its work judged the way its
//! attempt's close will judge it, while the session can still act on the
//! answer.
//!
//! A CLI asks before its session stops, and the engine answers with the
//! judgement the close would make of that session's work: a task
//! session's criteria, scope and request, or a node's documents and scope.
//! The session is held only when the close would not take the work and
//! would not settle it on its own. A grant or an escalation of the
//! session's request is the close's to decide, a criterion that cannot run
//! is nothing the session changes, and a criterion the session declared
//! impossible goes back to the planner. A held session is sent back to its
//! work with what stands in the way, and the log says so before the session
//! hears it.
//!
//! Never twice for one state of the work: a session that stops again
//! without changing anything has nothing better to give, and holding it
//! would only spend the run. And never once the session is being cut: a
//! stop then is the end it was asked for.

use yunta_core::events::{
    CriterionResult, CriterionUnpassablePayload, EventPayload, SessionEvent, StopRefusal,
    StopRefusedPayload, StoredEvent, TaskCheckAnsweredPayload, TaskCheckStartedPayload, TaskEvent,
};
use yunta_core::protected::Closing;
use yunta_core::rail::RailAnswer;
use yunta_core::{ContentHash, RunTool, SessionId, TaskId};

use super::host::TaskAccess;
use super::session::{RunToolError, SessionTools};
use super::verdicts;
use crate::replay::RunView;
use crate::scope::ScopeCheckResult;
use crate::scope_expansion::{read_request, GrantWindow, ScopeExpansionRequest};
use crate::task_cycle::{
    judge, Documents, JudgeEnv, Judgement, Refusals, Settle, Sitting, TaskCycleError, Work,
};
use std::path::Path;

impl SessionTools {
    /// Whether the session may stop now.
    pub(super) async fn before_stop(&self, session: &SessionId) -> RailAnswer {
        let judgement = match self
            .judge_session(Settle::Dry, std::slice::from_ref(session))
            .await
        {
            Ok(judgement) => judgement,
            Err(error) => {
                tracing::debug!(%error, "a stop went unjudged");
                return RailAnswer::Proceed;
            }
        };
        let state = judgement.state();
        if self.stop.is_cancelled() || !judgement.holds_a_stop() {
            return RailAnswer::Proceed;
        }
        let seen = match self.refused_before(session, &state).await {
            Ok(seen) => seen,
            Err(error) => {
                tracing::debug!(%error, "a stop's earlier refusals went unread");
                return RailAnswer::Proceed;
            }
        };
        if seen {
            return RailAnswer::Proceed;
        }
        let payload = EventPayload::Session(SessionEvent::StopRefused(StopRefusedPayload {
            session_id: session.clone(),
            state,
            why: stop_reasons(&judgement),
            task_id: self.task.as_ref().map(|access| access.task.id.clone()),
        }));
        // A refusal the log does not hold could be repeated forever: the
        // session stops instead.
        if let Err(error) = self.append(payload).await {
            tracing::debug!(%error, "a refused stop went unrecorded");
            return RailAnswer::Proceed;
        }
        // The text leaves the engine, so the secrets the run holds are taken
        // out of it, as they are out of the log.
        let text = verdicts::stop_text(&judgement, &self.called(RunTool::RequestScopeExpansion));
        RailAnswer::Refuse {
            reason: self.host.redactor.text(&text),
        }
    }

    /// A task session's work judged on its checkout as its attempt's close
    /// judges it, and kept on the log as one check of the task.
    pub(super) async fn judge_task(
        &self,
        access: &TaskAccess,
        settle: Settle,
        sessions: &[SessionId],
    ) -> Result<Judgement, RunToolError> {
        let reads = self.task_reads(access, sessions).await?;
        let tree = self.tree_now(&access.unit.worktree).await?;
        let (_turn, stop) = access.checks.take(tree, &self.stop, &access.index).await;
        self.append(EventPayload::Tasks(TaskEvent::CheckStarted(
            TaskCheckStartedPayload {
                task_id: access.task.id.clone(),
            },
        )))
        .await?;
        let started = std::time::Instant::now();
        let env = JudgeEnv {
            memo: &self.host.memo,
            protected: &access.protected,
            closing: Closing::Every,
            supervision: self.host.supervision(&stop),
        };
        let sitting = task_sitting(access, &reads, sessions, &self.host.run_dir, env);
        let judgement = judge(sitting, settle).await;
        access.checks.judged();
        if stop.is_cancelled() && !self.stop.is_cancelled() {
            return Err(RunToolError::Superseded);
        }
        let judgement = judgement.map_err(|source| RunToolError::Check { source })?;
        let results = self.recorded(&access.task.id, &judgement.criteria).await?;
        self.append(answered(
            &access.task.id,
            &judgement,
            results,
            started.elapsed(),
        ))
        .await?;
        Ok(judgement)
    }

    /// What a task's judgement reads before its turn. The log is read only
    /// when there is a request to decide, or a session whose refused writes
    /// are read.
    async fn task_reads(
        &self,
        access: &TaskAccess,
        sessions: &[SessionId],
    ) -> Result<TaskReads, RunToolError> {
        let request = read_request(&access.unit.worktree)
            .await
            .map_err(|source| RunToolError::Check {
                source: TaskCycleError::ScopeExpansion {
                    task: access.task.id.clone(),
                    source,
                },
            })?;
        let deciding = request.is_some() || !sessions.is_empty();
        let view = RunView::of(match deciding {
            true => self.events().await?,
            false => Vec::new(),
        });
        let window = deciding.then(|| GrantWindow::seeded(view.state.grants.granted()));
        let declared = access.declared.lock().await.clone();
        Ok(TaskReads {
            request,
            view,
            window,
            declared,
        })
    }

    /// A node's own session judged as its close judges it: the documents it
    /// hands over, and the diff against the scope it declares.
    pub(super) async fn judge_node(&self, settle: Settle) -> Result<Judgement, RunToolError> {
        let Some(access) = self.node_scope.as_deref() else {
            return self.judge_unscoped().await;
        };
        let view = RunView::of(self.events().await?);
        let from = view
            .state
            .nodes
            .from_tree(&self.node)
            .cloned()
            .ok_or(RunToolError::NoStartingTree)?;
        let env = JudgeEnv {
            memo: &self.host.memo,
            protected: &access.protected,
            closing: Closing::Every,
            supervision: self.host.supervision(&self.stop),
        };
        let sitting = Sitting {
            work: Work {
                checkout: &self.cwd,
                from: &from,
                staged: access.staged.get().map_or(&[], Vec::as_slice),
            },
            task: None,
            scope: &access.scope,
            request: None,
            refused: Refusals::None,
            window: None,
            bounds: None,
            declared: None,
            documents: Some(self.documents(&view.events)),
            env,
        };
        judge(sitting, settle)
            .await
            .map_err(|source| RunToolError::Check { source })
    }

    /// A node that declares no scope owes its close no audit, so the
    /// documents it hands over are all its judgement holds it to. The log is
    /// read only when it hands one over, and its tree only when the close
    /// would refuse one: a stop that owes nothing goes on nothing more.
    async fn judge_unscoped(&self) -> Result<Judgement, RunToolError> {
        let events = match Documents::any_handed(&self.declared, &self.node_kind) {
            true => self.events().await?,
            false => Vec::new(),
        };
        let documents = self.documents(&events).faults().await;
        let tree = match documents.is_empty() {
            true => None,
            false => Some(self.tree_now(&self.cwd).await?),
        };
        Ok(Judgement {
            criteria: Vec::new(),
            waiting: Vec::new(),
            scope: ScopeCheckResult::default(),
            request: None,
            documents,
            unpassable: None,
            tree,
        })
    }

    /// The documents this node's session hands over, verified against `events`.
    fn documents<'a>(&'a self, events: &'a [StoredEvent]) -> Documents<'a> {
        Documents {
            declared: &self.declared,
            node: &self.node,
            node_kind: &self.node_kind,
            run_dir: &self.host.run_dir,
            max_artifact_bytes: self.host.max_artifact_bytes,
            events,
        }
    }

    /// Whether `session` was sent back already with its work at `state`.
    async fn refused_before(
        &self,
        session: &SessionId,
        state: &ContentHash,
    ) -> Result<bool, RunToolError> {
        let view = RunView::of(self.events().await?);
        Ok(view.state.nodes.stop_refused_on(&self.node, session, state))
    }
}

/// What a task's judgement reads before its turn.
struct TaskReads {
    /// The request the session left in its checkout, if it left one.
    request: Option<ScopeExpansionRequest>,
    /// The run's log, read when a request is decided or refused writes are.
    view: RunView,
    /// The grant window a request counts against, seeded from the log.
    window: Option<GrantWindow>,
    /// The criterion the session declared impossible, as the log stands.
    declared: Option<CriterionUnpassablePayload>,
}

/// The sitting a task session's work is judged on: its task and checkout,
/// the scope it answers to, and what `reads` holds.
fn task_sitting<'a>(
    access: &'a TaskAccess,
    reads: &'a TaskReads,
    sessions: &'a [SessionId],
    run_dir: &'a Path,
    env: JudgeEnv<'a>,
) -> Sitting<'a> {
    Sitting {
        work: Work {
            checkout: access.unit.worktree.as_path(),
            from: &access.unit.from,
            staged: access.staged.get().map_or(&[], Vec::as_slice),
        },
        task: Some(&access.task),
        scope: &access.scope,
        request: reads.request.as_ref(),
        refused: Refusals::Log {
            events: &reads.view.events,
            sessions,
            run_dir,
        },
        window: reads.window.as_ref(),
        bounds: reads.window.is_some().then(|| access.scope_bounds()),
        declared: reads.declared.as_ref(),
        documents: None,
        env,
    }
}

/// What keeps the judged work from closing, each reason once: the criteria
/// that are red or waiting, then the documents the close would refuse, then
/// the changes outside the scope.
fn stop_reasons(judgement: &Judgement) -> Vec<StopRefusal> {
    let mut why = Vec::new();
    if !judgement.waiting.is_empty() || judgement.criteria.iter().any(|run| run.exit_code != 0) {
        why.push(StopRefusal::CriteriaRed);
    }
    for fault in &judgement.documents {
        if !why.contains(&fault.refusal) {
            why.push(fault.refusal);
        }
    }
    if !judgement.scope.violations.is_empty() {
        why.push(StopRefusal::OutsideScope);
    }
    why
}

/// The answer a check of `task` gave, as the log keeps it.
fn answered(
    task: &TaskId,
    judgement: &Judgement,
    results: Vec<CriterionResult>,
    took: std::time::Duration,
) -> EventPayload {
    EventPayload::Tasks(TaskEvent::CheckAnswered(TaskCheckAnsweredPayload {
        task_id: task.clone(),
        closes: judgement.closes(),
        results,
        waiting: judgement.waiting.clone(),
        outside_scope: judgement.scope.violations.clone(),
        denied: judgement.scope.denied.clone(),
        duration_ms: took.as_millis().min(u64::MAX as u128) as u64,
    }))
}
