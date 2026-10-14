//! The failures the round-trip guard reads back: one sample of each variant,
//! in the shape the log holds it.

use yunta_core::diagnostic::{DocumentRef, Report};
use yunta_core::inputs::InputsProblem;
use yunta_core::shown::Flaw;
use yunta_core::workflow::JoinPolicy;
use yunta_core::{
    AgentName, ArtifactKind, CommitSha, ExecutorName, FindingId, InputName, NodeId, OptionId,
    RunnerName, SkillName,
};

use super::*;

/// One sample of each failure that carries no typed cause of its own, and of
/// the two that a session or a frozen input raises.
pub(super) fn plain_failure_samples() -> Vec<Failure> {
    vec![
        Failure::Message {
            outcome: "exit 1".into(),
        },
        Failure::artifacts(Vec::new()),
        Failure::session_died(AdapterId::from_static("mock"), None),
        Failure::scope_violated(vec!["clippy.toml".into()]),
        Failure::scope_requested(vec![ScopeGlob::from("src/a.rs")], "needs it"),
        Failure::ScopeOwed { owed: Vec::new() },
        Failure::unset(ConfigKey::Runner),
        Failure::unchanged(
            1,
            Failure::Message {
                outcome: "regression: exit 101".into(),
            },
        ),
        Failure::paths_denied(vec!["secret.env".into()], Vec::new()),
        Failure::exited(CommandExit {
            origin: None,
            code: 1,
            tail: vec!["exit 1".into()],
            output: None,
        }),
        Failure::PlanUnpassable {
            unpassable: vec![UnpassableTask {
                task_id: "task-u".into(),
                cmd: "false".to_string(),
                why: "false exits 1".to_string(),
            }],
        },
        Failure::SessionNotStopped {
            session_not_stopped: "kill refused".into(),
        },
        Failure::FrozenInput {
            frozen_input: "agents/reviewer.md".into(),
        },
    ]
}

/// One sample of a refusal and of an account's limit.
pub(super) fn session_and_limit_samples() -> Vec<Failure> {
    vec![
        Failure::SessionRefused {
            session_refused: SessionRefusal {
                adapter: AdapterId::from_static("mock"),
                cause: RefusalCause::Authentication,
                message: "Invalid API key".into(),
            },
        },
        Failure::Limit {
            limit: AccountLimit {
                source: LimitSource::Forge,
                resets_at: None,
                message: "rate limited".into(),
            },
        },
    ]
}

/// One sample of a loop's stops, of a red check, and of a child run's
/// failure both known and unknown.
pub(super) fn loop_check_and_child_samples() -> Vec<Failure> {
    vec![
        Failure::TasksStopped {
            stopped: vec![TaskStop {
                task_id: TaskId::from_static("greet"),
                cause: TaskCause::ScopeDecisionOwed,
            }],
        },
        Failure::Red {
            red: RedCheck::Command {
                cmd: "false".into(),
                exit_code: 1,
                conclusion: None,
                output: None,
            },
        },
        Failure::ChildFailed {
            child_run: RunId::from("run-child-1"),
            child_failure: Some(Box::new(Failure::Limit {
                limit: AccountLimit {
                    source: LimitSource::Forge,
                    resets_at: None,
                    message: "rate limited".into(),
                },
            })),
        },
        Failure::ChildFailed {
            child_run: RunId::from("run-child-1"),
            child_failure: None,
        },
    ]
}

/// The samples a session, a loop and a node raise from a typed cause.
pub(super) fn typed_cause_samples() -> Vec<Failure> {
    vec![
        Failure::TimedOut {
            after_ms: 1000,
            timed: Timed::Session,
        },
        Failure::TimedOut {
            after_ms: 1000,
            timed: Timed::Executor {
                executor: ExecutorName::from_static("probe"),
            },
        },
        Failure::TokenBudget {
            used: 150,
            max: 100,
        },
        Failure::GateChose {
            node: yunta_core::NodeId::from("gate"),
            option: OptionId::from_static("retry"),
            target: NodeId::from_static("build"),
            said: None,
        },
        Failure::GateChose {
            node: yunta_core::NodeId::from("gate"),
            option: OptionId::from_static("retry"),
            target: NodeId::from_static("build"),
            said: Some("keep the cache".into()),
        },
        Failure::TestsAccepted {
            task: TaskId::from_static("T001"),
            owed_by: NodeId::from_static("build"),
        },
        Failure::LoopCapped {
            node: yunta_core::NodeId::from("loop"),
            loop_cap: 4,
        },
    ]
}

/// The artifact and template causes, one sample of each shape they carry.
pub(super) fn artifact_and_template_samples() -> Vec<Failure> {
    vec![
        Failure::Template {
            template: yunta_core::template::template_variables("{{run.directory}}").unwrap_err(),
            input: None,
        },
        Failure::Template {
            template: yunta_core::template::template_variables("{{run.directory}}").unwrap_err(),
            input: Some(InputName::from_static("goal")),
        },
        Failure::ArtifactUnreadable {
            artifact: ArtifactId::Opaque {
                name: "report.md".into(),
            },
            problem: ObjectProblem::Corrupt {
                hash: yunta_core::sha256_hex(b"accepted bytes"),
                found: yunta_core::sha256_hex(b"other bytes"),
            },
        },
        Failure::ArtifactName {
            name: "../escaped.md".into(),
            problem: NameProblem::OutsideRun,
        },
        Failure::ArtifactName {
            name: "tasks.yaml".into(),
            problem: NameProblem::Reserved {
                identity: yunta_core::ReservedIdentity::Kind(yunta_core::ArtifactKind::Tasks),
            },
        },
    ]
}

/// A mount that cannot bring its artifact in: the child run cannot hand over
/// its object, or its log cannot be read.
pub(super) fn mount_samples() -> Vec<Failure> {
    vec![
        Failure::MountUnreachable {
            error: Box::new(MountError::Unreadable {
                artifact: yunta_core::ArtifactRefId::Name {
                    name: "report.md".into(),
                },
                node: yunta_core::NodeId::from_static("plan"),
                run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
                source: ObjectProblem::Missing {
                    hash: yunta_core::sha256_hex(b"report"),
                },
            }),
        },
        Failure::MountUnreachable {
            error: Box::new(MountError::Unlogged {
                artifact: yunta_core::ArtifactRefId::Name {
                    name: "report.md".into(),
                },
                node: yunta_core::NodeId::from_static("plan"),
                run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
                source: "the store is offline".into(),
            }),
        },
    ]
}

/// The confinement a session could not be built with, and a frozen read the node asked for.
pub(super) fn confinement_and_frozen_samples() -> Vec<Failure> {
    vec![
        Failure::Confinement {
            confinement: yunta_core::Unbuildable::NetworkUnclosable,
        },
        Failure::Confinement {
            confinement: yunta_core::Unbuildable::SharedDirInCheckout {
                dir: "/repo/shared".into(),
            },
        },
        Failure::FrozenRead {
            frozen_read: FrozenRead::Prompt {
                node: yunta_core::NodeId::from_static("plan"),
                path: "prompts/plan.md".into(),
            },
        },
    ]
}

/// The samples a composed run, a capability, an engine step, a command and
/// a skill raise.
pub(super) fn engine_step_samples() -> Vec<Failure> {
    vec![
        Failure::NestedTooDeep {
            depth: 5,
            max_depth: 4,
            workflow: "sub".into(),
        },
        Failure::ChildFailed {
            child_run: RunId::from_static("01AAAAAAAAAAAAAAAAAAAAAAAA"),
            child_failure: Some(Box::new(Failure::TimedOut {
                after_ms: 1000,
                timed: Timed::Session,
            })),
        },
        Failure::GateArtifactMissing {
            node: yunta_core::NodeId::from("gate"),
            gate_artifact: ArtifactId::Opaque {
                name: "report.md".into(),
            },
        },
        Failure::CapabilityMissing {
            node: yunta_core::NodeId::from("implement"),
            capability: Capability::CustomAgents,
            adapter: AdapterId::from_static("mock"),
            used_for: None,
            agent: None,
        },
        Failure::CapabilityMissing {
            node: yunta_core::NodeId::from("implement"),
            capability: Capability::CustomAgents,
            adapter: AdapterId::from_static("mock"),
            used_for: Some(CapabilityUse::TypedArtifact {
                kind: ArtifactKind::Tasks,
            }),
            agent: Some(AgentName::from_static("reviewer")),
        },
        Failure::EngineFailed {
            engine_error: "cannot read the run's log".into(),
        },
        Failure::CommandDenied {
            denied_rule: "commands.deny: rm".into(),
        },
        Failure::SkillMissing {
            missing_skill: SkillName::from_static("review"),
            searched: vec!["/repo/.claude/skills".into()],
        },
    ]
}

/// The samples a parallel group, a gate a person answers and a review raise.
pub(super) fn join_gate_and_review_samples() -> Vec<Failure> {
    vec![
        Failure::ChildrenFailed {
            join: JoinPolicy::All,
            children_failed: vec![NodeId::from_static("left"), NodeId::from_static("right")],
        },
        Failure::ChildrenFailed {
            join: JoinPolicy::Any,
            children_failed: vec![NodeId::from_static("left")],
        },
        Failure::Uncertain {
            uncertain: NodeId::from_static("build"),
        },
        Failure::Rejected {
            rejected_by: "eulke".into(),
            on: Approval::Console,
        },
        Failure::SentBack {
            sent_back: vec![Flaw::UnrunSpecFile {
                task: TaskId::from_static("T001"),
                path: "tests/a.rs".into(),
            }],
        },
        Failure::FindingsOpen {
            at_or_above: FindingSeverity::Major,
            open_findings: vec![FindingId::from_static("f1")],
        },
        Failure::Interrupted {
            interrupted_by: Interruption::SiblingWon,
        },
        Failure::ChangesRequested {
            requested_by: "reviewer".into(),
            reviewed_sha: CommitSha::from("deadbeef"),
            comments: 2,
        },
        Failure::PullRequestClosed {
            closed_pull_request: None,
        },
        Failure::PullRequestClosed {
            closed_pull_request: Some("https://example.test/pull/1".into()),
        },
    ]
}

/// The samples a loop and a node's runner raise when they cannot go on.
pub(super) fn loop_and_runner_samples() -> Vec<Failure> {
    vec![
        Failure::LoopStalled {
            not_ready: vec![TaskId::from_static("T002")],
        },
        Failure::NoTasksDocument {
            tasks_for: NodeId::from_static("loop"),
        },
        Failure::RunnerUnresolved {
            runner: RunnerName::from_static("local"),
            runner_refusal: RunnerRefusal::Unknown,
        },
        Failure::RunnerUnresolved {
            runner: RunnerName::from_static("local"),
            runner_refusal: RunnerRefusal::NoCandidate {
                tried: vec![AdapterId::from_static("codex")],
            },
        },
    ]
}

/// One sample of each refusal a composed workflow can raise.
pub(super) fn refusal_samples() -> Vec<Failure> {
    let report = Report::new(
        DocumentRef::new(ArtifactKind::Tasks, "sub/plan.yaml"),
        Vec::new(),
    );
    vec![
        Failure::ChildRefused {
            workflow: "sub".into(),
            refusal: Box::new(ChildRefusal::Unresolved {
                problem: "no workflow is called `sub`".into(),
            }),
        },
        Failure::ChildRefused {
            workflow: "sub".into(),
            refusal: Box::new(ChildRefusal::Invalid { report }),
        },
        Failure::ChildRefused {
            workflow: "sub".into(),
            refusal: Box::new(ChildRefusal::FailsCheck {
                rules: vec!["composition_fails".into()],
            }),
        },
        Failure::ChildRefused {
            workflow: "sub".into(),
            refusal: Box::new(ChildRefusal::Inputs {
                problem: InputsProblem::Missing {
                    name: InputName::from_static("goal"),
                },
            }),
        },
        Failure::ChildRefused {
            workflow: "sub".into(),
            refusal: Box::new(ChildRefusal::Inputs {
                problem: InputsProblem::DocumentUnreadable {
                    name: InputName::from_static("plan"),
                    path: "plan.md".into(),
                    label: ArtifactKind::Tasks.label().to_string(),
                    detail: "is a directory".into(),
                },
            }),
        },
    ]
}
