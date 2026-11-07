//! What a page says about a run that its document says too, as data: how far it has come, what
//! it spent its time on, what it changed, where its work is and what it opened.

use std::collections::BTreeMap;

use yunta_core::{Isolation, Manifest, RunId};
use yunta_replay::frame::{RunFrame, TaskCounter};

use crate::run_page::pull_requests_of;

/// How far a run has come, in numbers.
#[derive(serde::Serialize)]
pub(crate) struct CountsJson {
    nodes: NodeCountsJson,
    /// Absent for a run with no tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    tasks: Option<TaskCountsJson>,
}

impl CountsJson {
    /// The run's nodes and tasks, each counted the way its frame counts them.
    pub(crate) fn of(frame: &RunFrame) -> Self {
        Self {
            nodes: NodeCountsJson {
                total: frame.flow.total,
                by: frame
                    .flow
                    .by
                    .iter()
                    .map(|(standing, count)| (standing.word(), *count))
                    .collect(),
                skipped: frame.flow.skipped,
                left_out: frame.flow.left_out,
            },
            tasks: frame.tasks.as_ref().map(TaskCountsJson::of),
        }
    }
}

/// Where a run's nodes stand.
#[derive(serde::Serialize)]
struct NodeCountsJson {
    /// Everything this run will do, what its mode and the project left out aside.
    total: usize,
    /// How many stand at each word, by the word every surface says.
    by: BTreeMap<&'static str, usize>,
    /// Left out by the mode.
    skipped: usize,
    /// Left out because the project lacks what they need.
    left_out: usize,
}

/// Where a run's tasks stand, in the words a task is called by.
#[derive(serde::Serialize)]
struct TaskCountsJson {
    total: usize,
    done: usize,
    failed: usize,
    running: usize,
    blocked: usize,
    to_go: usize,
}

impl TaskCountsJson {
    /// The run's tasks, counted the way its frame counts them.
    fn of(tasks: &TaskCounter) -> Self {
        Self {
            total: tasks.total,
            done: tasks.done,
            failed: tasks.failed,
            running: tasks.running,
            blocked: tasks.blocked,
            to_go: tasks.to_go,
        }
    }
}

/// Where a run's work is.
#[derive(serde::Serialize)]
pub(crate) struct BranchJson {
    /// The branch it works on. Absent for a run in a person's own checkout.
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// The branch it started from.
    base: String,
}

impl BranchJson {
    /// The branch a run works on, when its manifest gives it one of its own, and the branch
    /// the manifest says it started from.
    pub(crate) fn of(manifest: &Manifest, run_id: &RunId) -> Self {
        Self {
            name: (manifest.isolation == Isolation::Worktree)
                .then(|| yunta_engine::run_branch(run_id)),
            base: manifest.base_branch.clone(),
        }
    }
}

/// A pull request a run opened.
#[derive(serde::Serialize)]
pub(crate) struct PullRequestJson {
    number: u64,
    url: String,
}

impl PullRequestJson {
    /// Every pull request the run's nodes opened, in the order the nodes declare them.
    pub(crate) fn all_of(frame: &RunFrame) -> Vec<Self> {
        pull_requests_of(frame)
            .into_iter()
            .map(|(number, url)| Self {
                number,
                url: url.to_string(),
            })
            .collect()
    }
}
