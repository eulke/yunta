//! The words of an engine step that failed on its own: the words its site
//! wrote around what its failing call reported, which is the one thing the
//! step says of itself. A sentence an older log stored reads as it was written.

use yunta_core::events::{EngineError, EngineStep};
use yunta_core::text;

/// The failure an engine step records, in the words a reader sees.
pub fn failed(error: &EngineError) -> String {
    match error {
        EngineError::Step { step, report } => {
            step_said(step, report.as_deref().unwrap_or_default())
        }
        EngineError::Logged(said) => said.as_str().to_string(),
    }
}

/// One step's words: a step about a child run, or one the run takes on its own.
fn step_said(step: &EngineStep, report: &str) -> String {
    match step {
        EngineStep::ChildRoots { .. }
        | EngineStep::ChildManifest { .. }
        | EngineStep::ChildCheckout { .. }
        | EngineStep::ChildBirth { .. }
        | EngineStep::ChildResume { .. }
        | EngineStep::ChildSuccessor { .. }
        | EngineStep::MountRead { .. }
        | EngineStep::ChildHandover { .. } => child_said(step, report),
        _ => run_said(step, report),
    }
}

/// The words of a step about a child run, with what its failing call
/// reported in the place the words give it.
fn child_said(step: &EngineStep, report: &str) -> String {
    match step {
        EngineStep::ChildRoots { workflow } => {
            format!("child run of `{workflow}` cannot freeze its state roots: {report}")
        }
        EngineStep::ChildManifest { workflow } => {
            format!("child workflow `{workflow}` cannot freeze its manifest: {report}")
        }
        EngineStep::ChildCheckout { child } => {
            format!("child run `{child}` cannot prepare its worktree: {report}")
        }
        EngineStep::ChildBirth { workflow } => {
            format!("child workflow `{workflow}` cannot be born: {report}")
        }
        EngineStep::ChildResume { child } => {
            format!("child run `{child}` cannot resume: {report}")
        }
        EngineStep::ChildSuccessor { child, mode } => format!(
            "child run `{child}` promoted toward `{mode}` but its successor could not be \
             created: {report}"
        ),
        EngineStep::MountRead {
            artifact,
            node,
            run,
        } => format!(
            "mount of the {artifact} from node `{node}`: run `{run}` cannot hand over its bytes"
        ),
        EngineStep::ChildHandover { artifact } => format!(
            "the {}: the child run cannot hand over its bytes: {report}",
            artifact.label()
        ),
        _ => unreachable!("routed here only for a step about a child run"),
    }
}

/// The words of a step the run takes on its own, with what its failing call
/// reported in the place the words give it.
fn run_said(step: &EngineStep, report: &str) -> String {
    match step {
        EngineStep::ExecutorStdin { executor } => {
            format!("failed to serialize executor `{executor}`'s stdin: {report}")
        }
        EngineStep::Context | EngineStep::DeriveFindings | EngineStep::FrozenRead => {
            report.to_string()
        }
        EngineStep::StagedTwice => {
            "the staged paths of this session were already recorded".to_string()
        }
        EngineStep::Push { head, remote } => {
            format!("pushing `{head}` to `{remote}`: {report}")
        }
        EngineStep::OpenPullRequest => format!("opening the pull request: {report}"),
        EngineStep::ForgeToken => "the forge the config declares is not reachable from this \
             machine: the variable its `token_env` names is not set"
            .to_string(),
        EngineStep::ScopeRequest | EngineStep::RunTools => report.to_string(),
        EngineStep::Land { node, conflicts } => format!(
            "landing `{node}` on the run's tree: git could not replay it over {}",
            text::counted(*conflicts, "path")
        ),
        _ => unreachable!("routed here only for a step the run takes on its own"),
    }
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{ArtifactId, EngineError, EngineStep};
    use yunta_core::{ArtifactRefId, ModeName, NodeId, RunId};

    use super::*;

    /// A child run, as a log names it.
    const CHILD: &str = "01AAAAAAAAAAAAAAAAAAAAAAAA";

    fn said(step: EngineStep, report: Option<&str>) -> String {
        failed(&EngineError::Step {
            step,
            report: report.map(str::to_string),
        })
    }

    #[test]
    fn a_push_or_a_pull_request_says_its_step_around_what_it_reported() {
        assert_eq!(
            said(
                EngineStep::Push {
                    head: "yunta/run/01".into(),
                    remote: "origin".into(),
                },
                Some("rejected"),
            ),
            "pushing `yunta/run/01` to `origin`: rejected"
        );
        assert_eq!(
            said(EngineStep::OpenPullRequest, Some("422")),
            "opening the pull request: 422"
        );
    }

    #[test]
    fn a_child_run_says_its_step_around_what_it_reported() {
        assert_eq!(
            said(
                EngineStep::ChildResume {
                    child: RunId::from_static(CHILD),
                },
                Some("busy"),
            ),
            format!("child run `{CHILD}` cannot resume: busy")
        );
        assert_eq!(
            said(
                EngineStep::ChildSuccessor {
                    child: RunId::from_static(CHILD),
                    mode: ModeName::from_static("review"),
                },
                Some("busy"),
            ),
            format!(
                "child run `{CHILD}` promoted toward `review` but its successor could not be \
                 created: busy"
            )
        );
    }

    #[test]
    fn a_step_that_reports_nothing_of_its_own_says_its_words_alone() {
        assert_eq!(
            said(EngineStep::ForgeToken, None),
            "the forge the config declares is not reachable from this machine: the variable \
             its `token_env` names is not set"
        );
        assert_eq!(
            said(EngineStep::StagedTwice, None),
            "the staged paths of this session were already recorded"
        );
        assert_eq!(
            said(
                EngineStep::Land {
                    node: NodeId::from("build"),
                    conflicts: 2,
                },
                None,
            ),
            "landing `build` on the run's tree: git could not replay it over 2 paths"
        );
    }

    #[test]
    fn a_mount_and_a_handover_name_the_artifact_they_could_not_hand_over() {
        assert_eq!(
            said(
                EngineStep::MountRead {
                    artifact: ArtifactRefId::Name {
                        name: "report.md".into(),
                    },
                    node: NodeId::from("plan"),
                    run: RunId::from_static(CHILD),
                },
                None,
            ),
            format!(
                "mount of the report.md from node `plan`: run `{CHILD}` cannot hand over its bytes"
            )
        );
        assert_eq!(
            said(
                EngineStep::ChildHandover {
                    artifact: ArtifactId::Opaque {
                        name: "report.md".into(),
                    },
                },
                Some("gone"),
            ),
            "the artifact `report.md`: the child run cannot hand over its bytes: gone"
        );
    }

    #[test]
    fn a_sentence_an_older_log_stored_reads_as_it_was_written() {
        let stored = EngineError::Logged("pushing `yunta/run/01` to `origin`: rejected".into());
        assert_eq!(
            failed(&stored),
            "pushing `yunta/run/01` to `origin`: rejected"
        );
    }
}
