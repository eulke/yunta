//! One example of every event kind, for a test that has to name them
//! all.
//!
//! Two crates need this list and neither can hold it: `yunta-core`'s own
//! tests cannot reach the engine's derivation, and the engine's cannot
//! reach a fixture that lives in another crate's test target. It lives
//! here, once, so the two ask about the same thirty-seven.

use std::collections::BTreeMap;

use yunta_core::events::*;
use yunta_core::{Capability, RunId, RunnerCandidate, ScopeExpansionMode};

/// One payload per kind, in the order the wire writes them.
pub fn all_kinds() -> Vec<EventPayload> {
    vec![
        EventPayload::Run(RunEvent::Created(RunCreatedPayload {
            manifest_hash: yunta_core::sha256_hex(b"sha256:abc"),
            inputs: BTreeMap::new(),
            mode: "default".into(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".into(),
        })),
        EventPayload::Node(NodeEvent::RunnerResolved(RunnerResolvedPayload {
            runner: "executor".into(),
            chosen: RunnerCandidate {
                adapter: "mock".into(),
                model: "mock-model".into(),
                agent: None,
            },
            discarded: vec![],
        })),
        EventPayload::Node(NodeEvent::BaselineCaptured(BaselineCapturedPayload {
            command: "cargo test --workspace".to_string(),
            results: BaselineResults {
                exit_code: 0,
                summary: "12 passed".to_string(),
            },
            hash: yunta_core::sha256_hex(b"sha256:def"),
        })),
        EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        EventPayload::Session(SessionEvent::Opened(AgentSessionOpenedPayload {
            session_id: "sess-1".into(),
            agent: None,
            model: Some("mock-model".into()),
            capabilities: Capabilities::default(),
        })),
        EventPayload::Session(SessionEvent::Message(AgentMessagePayload {
            message_type: AgentMessageType::Usage,
            tool_name: None,
            target_digest: None,
            input_tokens: Some(100),
            output_tokens: Some(50),
            cached_input_tokens: None,
            text: None,
        })),
        EventPayload::Artifacts(ArtifactEvent::Written(ArtifactWrittenPayload {
            path: "artifacts/tasks.yaml".into(),
            content_hash: yunta_core::sha256_hex(b"sha256:111"),
            artifact_kind: Some(yunta_core::ArtifactKind::Tasks),
        })),
        EventPayload::Node(NodeEvent::ContextAssembled(ContextAssembledPayload {
            task_id: None,
            sources: vec![ContextSourceRef {
                source_id: "files:docs".to_string(),
                kind: "files".to_string(),
                content_hash: yunta_core::sha256_hex(b"sha256:333"),
            }],
            segment_hashes: BTreeMap::from([(
                "stable".to_string(),
                yunta_core::sha256_hex(b"222"),
            )]),
        })),
        EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
            task_id: "graph-cmd".into(),
            criteria: vec![Criterion {
                cmd: "cargo test -p yunta".to_string(),
                r#type: None,
            }],
            scope: vec!["crates/cli/**".to_string()],
            depends_on: vec![],
        })),
        EventPayload::Node(NodeEvent::CriteriaChecked(CriteriaCheckedPayload {
            task_id: "graph-cmd".into(),
            phase: Phase::Pre,
            results: vec![CriterionResult {
                cmd: "cargo test -p yunta".to_string(),
                exit_code: 1,
                r#type: None,
                reused: false,
                duration_ms: None,
            }],
        })),
        EventPayload::Tasks(TaskEvent::StatusChanged(TaskStatusChangedPayload::done(
            "graph-cmd".into(),
            42.into(),
            "deadbeef".into(),
        ))),
        EventPayload::Node(NodeEvent::ScopeChecked(ScopeCheckedPayload {
            task_id: Some("graph-cmd".into()),
            diff: vec!["crates/cli/src/graph.rs".into()],
            violations: vec![],
        })),
        EventPayload::Scope(ScopeEvent::Requested(ScopeExpansionRequestedPayload {
            task_id: "graph-cmd".into(),
            paths: vec!["crates/cli/src/**".to_string()],
            reason: "need to touch main.rs too".to_string(),
            proposed_criterion: None,
            proposed_criterion_precheck: None,
        })),
        EventPayload::Scope(ScopeEvent::Granted(ScopeExpansionGrantedPayload {
            task_id: "graph-cmd".into(),
            decided_by: Decider::Rule,
            mode: ScopeExpansionMode::Rules,
            count_this_run: 1,
            paths: vec!["crates/cli/src/**".to_string()],
        })),
        EventPayload::Scope(ScopeEvent::Denied(ScopeExpansionDeniedPayload {
            task_id: "graph-cmd".into(),
            decided_by: Decider::Person { id: "eulke".into() },
            mode: ScopeExpansionMode::Ask,
            count_this_run: 2,
            denial_reason: Some("out of declared scope".to_string()),
        })),
        EventPayload::Node(NodeEvent::Finished(NodeFinishedPayload::new(
            "criteria green",
            TokenUsage {
                input: 10,
                output: 5,
                cached: None,
            },
        ))),
        EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
            Failure::message("criteria red"),
            true,
            TokenUsage::default(),
        ))),
        EventPayload::Node(NodeEvent::HookExecuted(HookExecutedPayload {
            phase: HookPhase::After,
            command: "cargo fmt".to_string(),
            exit_code: 0,
        })),
        EventPayload::Node(NodeEvent::Rerouted(NodeReroutedPayload::new(
            "fix-lint".into(),
            yunta_core::events::RerouteCause(yunta_core::events::Failure::message("clippy failed")),
            yunta_core::events::RerouteOrigin::OnFailure,
            Some(1),
            Some(2),
        ))),
        EventPayload::Gates(GateEvent::Waiting(
            Escalation::published_to(
                "Ready to open the PR?",
                vec![Fact::labelled("published at", "example/repo#1")].into(),
                "https://github.com/example/repo/pull/1",
            )
            .expect("the summary states no fact the evidence holds")
            .into_payload(),
        )),
        EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Approved {
            by: "eulke".into(),
            sha: "deadbeef".into(),
        })),
        EventPayload::Gates(GateEvent::QuestionsAsked(
            QuestionsAskedPayload::new(
                yunta_core::sha256_hex(b"sha256:222"),
                vec!["q1".into()],
                TokenUsage {
                    input: 30,
                    output: 12,
                    cached: None,
                },
            )
            .expect("one question is a question"),
        )),
        EventPayload::Gates(GateEvent::QuestionsAnswered(QuestionsAnsweredPayload {
            answers_hash: yunta_core::sha256_hex(b"sha256:333"),
            channel: Channel::Tty,
            responder: Some("eulke".into()),
        })),
        EventPayload::Children(ChildEvent::LoopIteration(LoopIterationPayload {
            iteration: 3,
            until_result: false,
        })),
        EventPayload::Findings(FindingEvent::Posted(FindingPostedPayload {
            finding: Finding {
                id: "f-1".into(),
                severity: FindingSeverity::Major,
                title: "missing error handling".to_string(),
                location: "crates/cli/src/main.rs:10".to_string(),
                detail: "unwrap on a fallible call".to_string(),
                proposed_criterion: None,
            },
        })),
        EventPayload::Findings(FindingEvent::Updated(FindingUpdatedPayload {
            finding: Finding {
                id: "f-1".into(),
                severity: FindingSeverity::Blocking,
                title: "missing error handling".to_string(),
                location: "crates/cli/src/main.rs:10-14".to_string(),
                detail: "unwrap on a fallible call, reached on every run".to_string(),
                proposed_criterion: None,
            },
        })),
        EventPayload::Findings(FindingEvent::Withdrawn(FindingWithdrawnPayload {
            id: "f-1".into(),
            reason: "the call it named is gone".to_string(),
        })),
        EventPayload::Findings(FindingEvent::Refused(FindingRefusedPayload {
            operation: FindingOperation::Post,
            id: Some("f-3".into()),
            report: yunta_core::diagnostic::Report::new(
                yunta_core::diagnostic::DocumentRef::new(
                    yunta_core::ArtifactKind::Findings,
                    "yunta_post_finding",
                ),
                vec![
                    yunta_core::diagnostic::Diagnostic::new(
                        yunta_core::diagnostic::Subject::Document,
                        yunta_core::diagnostic::Problem::parse("severity", "unknown variant `big`"),
                    ),
                    yunta_core::diagnostic::Diagnostic::new(
                        yunta_core::diagnostic::Subject::Finding(
                            yunta_core::diagnostic::Named::new(
                                yunta_core::FindingId::try_from("f-3".to_string()).unwrap(),
                                0,
                            ),
                        ),
                        yunta_core::diagnostic::Problem::rule(
                            yunta_core::diagnostic::RuleCode::EmptyDetail,
                            "`detail` is empty",
                        ),
                    ),
                ],
            ),
        })),
        EventPayload::Artifacts(ArtifactEvent::Submitted(ArtifactSubmittedPayload {
            name: "plan.yaml".to_string(),
            artifact_kind: yunta_core::ArtifactKind::Tasks,
            outcome: SubmissionOutcome::Accepted {
                content_hash: yunta_core::sha256_hex(b"plan"),
            },
        })),
        EventPayload::Artifacts(ArtifactEvent::Accepted(ArtifactAcceptedPayload::new(
            ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Tasks,
            },
            yunta_core::sha256_hex(b"plan"),
            ArtifactOrigin::Inherited {
                run: RunId::from("run-parent"),
                producer: Some(yunta_core::NodeId::from("plan")),
            },
        ))),
        EventPayload::Run(RunEvent::PromotionSignaled(PromotionSignaledPayload {
            reason: "all quick-mode nodes green".to_string(),
            evidence: vec![Fact::labelled("criteria", "log")].into(),
            suggested_mode: "standard".into(),
        })),
        EventPayload::Children(ChildEvent::Created(ChildRunCreatedPayload {
            child_run_id: "run-child-1".into(),
            child_workflow_hash: yunta_core::sha256_hex(b"sha256:444"),
        })),
        EventPayload::Children(ChildEvent::Finished(ChildRunFinishedPayload::new(
            "run-child-1".into(),
            yunta_core::sha256_hex(b"sha256:444"),
            TerminalState::Done,
            TokenUsage::default(),
        ))),
        EventPayload::Session(SessionEvent::CapabilityDegraded(
            CapabilityDegradedPayload::new(
                Capability::ResumeSession,
                "mock".into(),
                yunta_core::events::Policy::FreshSession,
            ),
        )),
        EventPayload::Run(RunEvent::Paused(RunPausedPayload::recorded(
            "gate waiting".to_string(),
        ))),
        EventPayload::Run(RunEvent::Resumed(RunResumedPayload {
            resume_policy_applied: Some("restart_node".to_string()),
            policies: Vec::new(),
        })),
        EventPayload::Run(RunEvent::Finished(RunFinishedPayload {
            terminal_state: TerminalState::Done,
            metrics: RunMetrics {
                cptv: Some(0.42),
                tokens: TokenUsage {
                    input: 1000,
                    output: 500,
                    cached: Some(200),
                },
            },
        })),
    ]
}
