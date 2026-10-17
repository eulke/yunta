//! The engine steps that fail on their own: which step it was, and what its
//! failing call reported of itself. The log keeps the step as data, and a
//! surface writes the words around it.

use serde::{Deserialize, Serialize};

use crate::events::{ArtifactId, LoggedWords};
use crate::ids::{ExecutorName, ModeName, NodeId, RunId};
use crate::workflow::ArtifactRefId;

/// What a failed engine step records: the step, and what its failing call
/// reported of itself, or the sentence an older log stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum EngineError {
    Step {
        #[serde(flatten)]
        step: EngineStep,
        /// What the failing call reported, absent when the step has no error to report.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        report: Option<String>,
    },
    /// The sentence an older log stored, kept as written.
    Logged(LoggedWords),
}

/// The engine step that failed on its own, with what names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum EngineStep {
    /// The stdin an executor reads could not be serialized.
    ExecutorStdin { executor: ExecutorName },
    /// The node's context could not be resolved.
    Context,
    /// A session found its staged record already taken.
    StagedTwice,
    /// A child run's state roots could not be frozen.
    ChildRoots { workflow: String },
    /// A child workflow's manifest could not be frozen.
    ChildManifest { workflow: String },
    /// A child run's worktree could not be prepared.
    ChildCheckout { child: RunId },
    /// A child workflow could not be born: its frozen inputs could not be written.
    ChildBirth { workflow: String },
    /// A child run could not be resumed.
    ChildResume { child: RunId },
    /// A promoted child run's successor could not be created toward `mode`.
    ChildSuccessor { child: RunId, mode: ModeName },
    /// A child run could not hand over the bytes a mount of its artifact names.
    MountRead {
        artifact: ArtifactRefId,
        node: NodeId,
        run: RunId,
    },
    /// The findings a node's artifacts declare could not be derived.
    DeriveFindings,
    /// A child run's store could not hand over the bytes an artifact names.
    ChildHandover { artifact: ArtifactId },
    /// The frozen state a session reads could not be read.
    FrozenRead,
    /// A branch could not be pushed to its remote.
    Push { head: String, remote: String },
    /// The pull request could not be opened on the forge.
    OpenPullRequest,
    /// The variable the forge's `token_env` names is not set.
    ForgeToken,
    /// A session's request for more scope could not be read from its checkout.
    ScopeRequest,
    /// The listener that carries a node's run tools did not start.
    RunTools,
    /// A node's changes could not be landed on the run's tree: git could not
    /// replay them over `conflicts` paths.
    Land { node: NodeId, conflicts: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steps of a composed run: its children, and the artifacts it mounts from them.
    fn child_steps() -> Vec<EngineStep> {
        let child = || RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA");
        vec![
            EngineStep::ChildRoots {
                workflow: "sub".into(),
            },
            EngineStep::ChildManifest {
                workflow: "sub".into(),
            },
            EngineStep::ChildCheckout { child: child() },
            EngineStep::ChildBirth {
                workflow: "sub".into(),
            },
            EngineStep::ChildResume { child: child() },
            EngineStep::ChildSuccessor {
                child: child(),
                mode: ModeName::from_static("review"),
            },
            EngineStep::MountRead {
                artifact: ArtifactRefId::Name {
                    name: "report.md".into(),
                },
                node: NodeId::from_static("plan"),
                run: child(),
            },
            EngineStep::ChildHandover {
                artifact: ArtifactId::Opaque {
                    name: "report.md".into(),
                },
            },
        ]
    }

    /// The steps a run takes on its own: its executors, its frozen reads,
    /// its pushes and pull requests, its scope, its tools and its tree.
    fn run_steps() -> Vec<EngineStep> {
        vec![
            EngineStep::ExecutorStdin {
                executor: ExecutorName::from_static("probe"),
            },
            EngineStep::Context,
            EngineStep::StagedTwice,
            EngineStep::DeriveFindings,
            EngineStep::FrozenRead,
            EngineStep::Push {
                head: "yunta/run/01".into(),
                remote: "origin".into(),
            },
            EngineStep::OpenPullRequest,
            EngineStep::ForgeToken,
            EngineStep::ScopeRequest,
            EngineStep::RunTools,
            EngineStep::Land {
                node: NodeId::from_static("build"),
                conflicts: 2,
            },
        ]
    }

    #[test]
    fn every_step_reads_back_as_the_step_it_was_written_as() {
        for step in child_steps().into_iter().chain(run_steps()) {
            let failure = EngineError::Step {
                step,
                report: Some("the step failed".into()),
            };
            let written = serde_json::to_value(&failure).unwrap();
            let read: EngineError = serde_json::from_value(written.clone()).unwrap();
            assert_eq!(read, failure, "{written}");
        }
    }

    #[test]
    fn a_step_is_written_flat_beside_its_report() {
        let failure = EngineError::Step {
            step: EngineStep::Push {
                head: "yunta/run/01".into(),
                remote: "origin".into(),
            },
            report: Some("rejected".into()),
        };
        assert_eq!(
            serde_json::to_value(&failure).unwrap(),
            serde_json::json!({
                "step": "push",
                "head": "yunta/run/01",
                "remote": "origin",
                "report": "rejected",
            })
        );
    }

    #[test]
    fn a_step_with_no_report_writes_its_name_alone() {
        let failure = EngineError::Step {
            step: EngineStep::ForgeToken,
            report: None,
        };
        assert_eq!(
            serde_json::to_value(&failure).unwrap(),
            serde_json::json!({ "step": "forge_token" })
        );
    }

    #[test]
    fn a_sentence_an_older_log_stored_reads_as_logged() {
        let read: EngineError = serde_json::from_value(serde_json::json!(
            "pushing `yunta/run/01` to `origin`: rejected"
        ))
        .unwrap();
        assert_eq!(
            read,
            EngineError::Logged("pushing `yunta/run/01` to `origin`: rejected".into())
        );
    }
}
