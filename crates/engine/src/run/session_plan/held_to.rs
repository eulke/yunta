//! What one session's work is held to, which its fence, its tools and
//! what they leave out all read.

use std::path::PathBuf;
use std::sync::Arc;

use yunta_core::ScopeGlob;

use super::SessionPlan;
use crate::run_tools::{NodeScopeAccess, TaskAccess};
use crate::task_cycle::SessionSetup;

/// What one session's work is held to: a loop's task, the node's own
/// scope, or nothing a tool could judge it by. Its fence, its tools and
/// what they leave out all read this one answer.
pub(super) enum HeldTo {
    Task(Arc<TaskAccess>),
    NodeScope(Arc<NodeScopeAccess>),
    Nothing,
}

impl HeldTo {
    pub(super) fn of(setup: &SessionSetup, plan: &SessionPlan<'_>) -> Self {
        match (&plan.task, &setup.node_scope) {
            (Some(task), _) => HeldTo::Task(task.clone()),
            (None, Some(access)) => HeldTo::NodeScope(access.clone()),
            (None, None) => HeldTo::Nothing,
        }
    }

    /// The globs the session may write under the worktree, when anything
    /// holds it to some.
    pub(super) fn scope(&self) -> Option<&[ScopeGlob]> {
        match self {
            HeldTo::Task(task) => Some(&task.scope),
            HeldTo::NodeScope(access) => Some(&access.scope),
            HeldTo::Nothing => None,
        }
    }

    /// What the session may never write: what the project denies to every
    /// run, and for a task's session what its task is denied besides —
    /// the files its tests live in.
    pub(super) fn denied<'a>(&'a self, project: &'a [ScopeGlob]) -> &'a [ScopeGlob] {
        match self {
            HeldTo::Task(task) => &task.denied,
            HeldTo::NodeScope(_) | HeldTo::Nothing => project,
        }
    }

    /// Whether the session may ask for more: a task session always may,
    /// and a node's own session when a person may widen its scope.
    pub(super) fn may_ask(&self) -> bool {
        match self {
            HeldTo::Task(_) => true,
            HeldTo::NodeScope(access) => access.may_ask,
            HeldTo::Nothing => false,
        }
    }

    /// Where what the adapter stages for itself is recorded, for the
    /// tools that audit this session's work to leave out.
    pub(super) fn staged(&self) -> Option<&std::sync::OnceLock<Vec<PathBuf>>> {
        match self {
            HeldTo::Task(task) => Some(&task.staged),
            HeldTo::NodeScope(access) => Some(&access.staged),
            HeldTo::Nothing => None,
        }
    }

    /// The two halves a session's tools are opened with.
    pub(super) fn parts(&self) -> (Option<Arc<TaskAccess>>, Option<Arc<NodeScopeAccess>>) {
        match self {
            HeldTo::Task(task) => (Some(task.clone()), None),
            HeldTo::NodeScope(access) => (None, Some(access.clone())),
            HeldTo::Nothing => (None, None),
        }
    }
}
