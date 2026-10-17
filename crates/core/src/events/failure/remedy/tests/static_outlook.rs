//! The outlooks of the failures whose remedies no fact in the log changes:
//! each cause is stuck on the config, the tree or a decision a person owes, or
//! offers the one retry that waits for what it names.

use crate::events::{
    Approval, ArtifactId, ChildRefusal, DeniedBy, DeniedCommand, DeniedRule, EngineError,
    EngineStep, FindingSeverity, FrozenRead, Interruption, MountError, NameProblem, ObjectProblem,
    RunnerRefusal, Timed,
};
use crate::ids::{FindingId, NodeId, OptionId, Responder, RunId, RunnerName, SkillName};
use crate::shown::Flaw;
use crate::template::{TemplateError, TemplateVar};
use crate::workflow::{ArtifactRefId, JoinPolicy};
use crate::{sha256_hex, Capability, CommitSha, Unbuildable};

use super::*;

/// Every failure in `failures` has the outlook `expected`, at a fixed time.
fn outlook_on(failures: Vec<Failure>, expected: Outlook) {
    let facts = facts_at(Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap());
    for failure in failures {
        assert_eq!(failure.remedies(&facts), expected, "{failure:?}");
    }
}

#[test]
fn a_frozen_input_that_changed_is_stuck_on_its_own_frozen_input() {
    outlook_on(
        vec![Failure::FrozenInput {
            frozen_input: "agents/reviewer.md".to_string(),
        }],
        Outlook::Stuck(Stuck::FrozenInput),
    );
}

#[test]
fn a_session_that_could_not_stop_offers_a_retry_that_waits_for_the_outside() {
    outlook_on(
        vec![Failure::SessionNotStopped {
            session_not_stopped: "kill refused".to_string(),
            release_failed: None,
        }],
        retry_waiting_for(Precondition::OutsideTreeChanged),
    );
}

#[test]
fn a_failure_the_frozen_config_causes_is_stuck_on_the_config_the_run_froze() {
    outlook_on(
        vec![
            Failure::Template {
                template: TemplateError::Undefined {
                    name: TemplateVar::RunDir,
                },
                input: None,
            },
            Failure::ArtifactName {
                name: "../escaped.md".into(),
                problem: NameProblem::OutsideRun,
            },
            Failure::NestedTooDeep {
                depth: 5,
                max_depth: 4,
                workflow: "sub".into(),
            },
            Failure::GateArtifactMissing {
                node: crate::NodeId::from("gate"),
                gate_artifact: ArtifactId::Opaque {
                    name: "report.md".into(),
                },
            },
        ],
        Outlook::Stuck(Stuck::FrozenConfig),
    );
}

#[test]
fn a_capability_child_or_read_the_run_froze_is_stuck_on_the_config_too() {
    outlook_on(
        vec![
            Failure::CapabilityMissing {
                node: crate::NodeId::from("implement"),
                capability: Capability::CustomAgents,
                adapter: AdapterId::from_static("mock"),
                used_for: None,
                agent: None,
            },
            Failure::SkillMissing {
                missing_skill: SkillName::from_static("review"),
                searched: Vec::new(),
            },
            Failure::ChildRefused {
                workflow: "sub".into(),
                refusal: Box::new(ChildRefusal::FailsCheck {
                    rules: vec!["composition_fails".into()],
                }),
            },
            Failure::Confinement {
                confinement: Unbuildable::NetworkUnclosable,
            },
            Failure::FrozenRead {
                frozen_read: FrozenRead::Unresolved {
                    problem: "no agent `reviewer`".into(),
                },
            },
        ],
        Outlook::Stuck(Stuck::FrozenConfig),
    );
}

#[test]
fn a_runner_no_config_defines_is_stuck_on_the_config_the_run_froze() {
    outlook_on(
        vec![Failure::RunnerUnresolved {
            runner: RunnerName::from_static("local"),
            runner_refusal: RunnerRefusal::Unknown,
        }],
        Outlook::Stuck(Stuck::FrozenConfig),
    );
}

#[test]
fn a_failure_the_outside_settles_waits_for_the_outside_to_change() {
    outlook_on(
        vec![
            Failure::TimedOut {
                after_ms: 1000,
                timed: Timed::Session,
            },
            Failure::ArtifactUnreadable {
                artifact: ArtifactId::Opaque {
                    name: "report.md".into(),
                },
                problem: ObjectProblem::Missing {
                    hash: sha256_hex(b"report"),
                },
            },
            Failure::MountUnreachable {
                error: Box::new(MountError::NoTerminalChild {
                    artifact: ArtifactRefId::Name {
                        name: "report.md".into(),
                    },
                    node: NodeId::from_static("plan"),
                }),
            },
            Failure::EngineFailed {
                engine_error: EngineError::Step {
                    step: EngineStep::FrozenRead,
                    report: Some("cannot read the run's log".into()),
                },
            },
            Failure::RunnerUnresolved {
                runner: RunnerName::from_static("local"),
                runner_refusal: RunnerRefusal::NoCandidate {
                    tried: vec![AdapterId::from_static("codex")],
                },
            },
        ],
        retry_waiting_for(Precondition::OutsideTreeChanged),
    );
}

#[test]
fn a_token_budget_waits_for_a_person_to_lift_the_budget_from_outside() {
    outlook_on(
        vec![Failure::TokenBudget {
            used: 150,
            max: 100,
        }],
        retry_waiting_for(Precondition::OutsideTreeChanged),
    );
}

#[test]
fn a_gate_child_or_loop_stop_waits_for_the_outside_to_change() {
    outlook_on(
        vec![
            Failure::ChildrenFailed {
                join: JoinPolicy::All,
                children_failed: vec![NodeId::from_static("left")],
            },
            Failure::ChildFailed {
                child_run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
                child_failure: None,
            },
            Failure::Uncertain {
                uncertain: NodeId::from_static("build"),
            },
            Failure::Interrupted {
                interrupted_by: Interruption::SiblingWon,
            },
            Failure::Rejected {
                rejected_by: Responder::from("eulke"),
                on: Approval::Console,
            },
            Failure::SentBack {
                sent_back: vec![Flaw::UnrunSpecFile {
                    task: TaskId::from_static("T001"),
                    path: "tests/a.rs".into(),
                }],
            },
            Failure::FindingsOpen {
                at_or_above: FindingSeverity::Blocking,
                open_findings: vec![FindingId::from_static("f1")],
            },
            Failure::ChangesRequested {
                requested_by: "reviewer".into(),
                reviewed_sha: CommitSha::from("deadbeef"),
                comments: 2,
            },
            Failure::PullRequestClosed {
                closed_pull_request: Some("https://example.test/pull/1".into()),
            },
            Failure::LoopStalled {
                not_ready: vec![TaskId::from_static("T002")],
            },
            Failure::NoTasksDocument {
                tasks_for: NodeId::from_static("loop"),
            },
            Failure::Message {
                outcome: "exit 1".into(),
            },
        ],
        retry_waiting_for(Precondition::OutsideTreeChanged),
    );
}

#[test]
fn a_loop_past_its_cap_waits_for_a_person_to_lift_the_cap() {
    outlook_on(
        vec![Failure::LoopCapped {
            node: crate::NodeId::from("loop"),
            loop_cap: 4,
        }],
        retry_waiting_for(Precondition::CapLifted),
    );
}

#[test]
fn a_denied_command_is_stuck_on_the_project_and_a_gate_choice_is_final() {
    outlook_on(
        vec![Failure::CommandDenied {
            denied_rule: DeniedRule::Command(DeniedCommand {
                command: "rm -rf x".into(),
                by: DeniedBy::Deny {
                    pattern: "rm *".into(),
                },
            }),
        }],
        Outlook::Stuck(Stuck::ProjectDenies),
    );
    outlook_on(
        vec![
            Failure::GateChose {
                node: crate::NodeId::from("gate"),
                option: OptionId::from_static("retry"),
                target: NodeId::from_static("build"),
                said: Some("retry with the cache off".into()),
            },
            Failure::TestsAccepted {
                task: TaskId::from_static("T001"),
                owed_by: NodeId::from_static("build"),
            },
        ],
        Outlook::Stuck(Stuck::Final),
    );
}
