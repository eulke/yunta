//! The tools that speak about the scope a session works to: what of its
//! node's work lies outside it, and how the session asks for it to be
//! widened.
//!
//! A session never widens its own scope. A request refused whatever the
//! work, for what the project protects or for the mode, is answered in the
//! call, with its reason, and logged at once. Any other request is written
//! as the request file the attempt's end reads, and decided there: for a
//! task by its evidence, its loop's rules or a person, for a node by a
//! person, from the menu of the failure the request becomes. One mechanism
//! with two intake surfaces, rather than a second policy living behind the
//! listener.

use serde::Serialize;
use serde_json::Value;
use yunta_core::events::Decider;
use yunta_core::expansion::{Decision, Denial};
use yunta_core::ScopeGlob;

use super::catalog::RunTool;
use super::host::TaskAccess;
use super::session::{RunToolError, SessionTools};
use super::verdicts::json;
use super::words;
use crate::replay::RunView;
use crate::run::scope_record::{self, AskedFor, Recording, Subject};
use crate::scope_expansion::{
    standing_refusal, write_request, Recorded, RequestError, ScopeExpansionRequest,
};
use crate::task_cycle::Settle;

impl SessionTools {
    /// What this node's close would find outside its scope if the
    /// session ended now: the same audit, from the tree the attempt's
    /// start recorded, against the scope the close holds it to.
    pub(super) async fn check_scope(&self) -> Result<String, RunToolError> {
        let access = self
            .node_scope
            .as_deref()
            .ok_or(RunToolError::NoNodeScope)?;
        let result = self.scope_audit().await?;
        json(&ScopeVerdict {
            within: result.violations.is_empty(),
            scope: &access.scope,
            outside_scope: result.violations,
        })
    }

    /// The audit this node's close would make of its work now: the dry
    /// judgement of the node, which moves no grant.
    pub(super) async fn scope_audit(&self) -> Result<crate::scope::ScopeCheckResult, RunToolError> {
        Ok(self.judge_session(Settle::Dry, &[]).await?.scope)
    }

    /// Asks for the scope to be widened. A task's request no work could get
    /// granted is answered here, with its reason; any other request is
    /// written for the attempt's end to decide.
    pub(super) async fn request_scope_expansion(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let answered_by_a_person = match (&self.task, &self.node_scope) {
            (Some(_), _) => false,
            (None, Some(access)) if access.may_ask => true,
            _ => return Err(RunToolError::NoScopeToWiden),
        };
        // The identical request object the file-based path uses, validated
        // by the same type it parses.
        let request: ScopeExpansionRequest = serde_json::from_value(Value::Object(args))
            .map_err(|source| RunToolError::InvalidRequest { source })?;
        if let Some(access) = self.task.as_deref() {
            if let Some(denial) = standing_refusal(&request, &access.scope_bounds()) {
                self.record_denial(access, &request, &denial).await?;
                return Ok(words::denied(&denial, &self.called(RunTool::CheckTask)));
            }
        }
        let next = words::request_next(answered_by_a_person, &self.called(RunTool::CheckScope));
        Ok(
            match write_request(&self.cwd, request, &self.host.scope_guard).await? {
                Recorded::Written => words::recorded(&next),
                Recorded::Amended { added } => words::amended(added, &next),
            },
        )
    }

    /// Records a request the project settled at intake: what the session
    /// asked, and the denial that refused it, with the finding that says
    /// why.
    async fn record_denial(
        &self,
        access: &TaskAccess,
        request: &ScopeExpansionRequest,
        denial: &Denial,
    ) -> Result<(), RunToolError> {
        let count = RunView::of(self.events().await?).state.grants.granted();
        let decision = Decision::Deny(denial.clone());
        scope_record::record(
            &self.log(),
            &self.node,
            Recording {
                subject: Subject::Task(&access.task.id),
                mode: Some(access.expansion.mode),
                by: Decider::Rule,
                asked: Some(AskedFor::Request(request)),
                decision: &decision,
                count,
                located: false,
                precheck: None,
                attempt: access.expansion.attempt,
                new_request: true,
            },
        )
        .await
        .map_err(|source| RunToolError::Record { source })
    }
}

impl From<RequestError> for RunToolError {
    /// A request under another criterion is the call's own refusal; a request
    /// file the engine cannot read, parse or write is the engine's failure.
    fn from(source: RequestError) -> Self {
        match source {
            RequestError::CriterionConflict { pending } => {
                RunToolError::CriterionConflict { pending }
            }
            source => RunToolError::Request { source },
        }
    }
}

/// A node's work against its scope, as `yunta_check_scope` answers it.
#[derive(Serialize)]
struct ScopeVerdict<'a> {
    /// True when nothing the node changed lies outside its scope.
    within: bool,
    /// What the node may write: what it declared plus what was granted.
    scope: &'a [ScopeGlob],
    /// What it changed outside that scope.
    outside_scope: Vec<std::path::PathBuf>,
}
