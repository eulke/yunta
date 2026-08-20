use std::collections::HashMap;

use yunta_core::events::*;
use yunta_core::{RunId, RunnerCandidate};

fn all_kinds() -> Vec<EventPayload> {
    vec![
        EventPayload::RunCreated(RunCreatedPayload {
            manifest_hash: "sha256:abc".to_string(),
            inputs: HashMap::new(),
            mode: "default".to_string(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".to_string(),
        }),
        EventPayload::RunnerResolved(RunnerResolvedPayload {
            role: "executor".to_string(),
            chosen: RunnerCandidate {
                adapter: "mock".to_string(),
                model: "mock-model".to_string(),
                agent: None,
            },
            discarded: vec![],
        }),
        EventPayload::BaselineCaptured(BaselineCapturedPayload {
            command: "cargo test --workspace".to_string(),
            results: BaselineResults {
                exit_code: 0,
                summary: "12 passed".to_string(),
            },
            hash: "sha256:def".to_string(),
        }),
        EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        EventPayload::AgentSessionOpened(AgentSessionOpenedPayload {
            session_id: "sess-1".into(),
            agent: None,
            model: "mock-model".to_string(),
            capabilities: Capabilities::default(),
        }),
        EventPayload::AgentMessage(AgentMessagePayload {
            message_type: AgentMessageType::Usage,
            tool_name: None,
            target_digest: None,
            input_tokens: Some(100),
            output_tokens: Some(50),
            cached_input_tokens: None,
            text: None,
        }),
        EventPayload::ArtifactWritten(ArtifactWrittenPayload {
            path: "artifacts/ledger.yaml".into(),
            content_hash: "sha256:111".to_string(),
            artifact_kind: Some(yunta_core::ArtifactKind::TaskLedger),
        }),
        EventPayload::ContextAssembled(ContextAssembledPayload {
            task_id: None,
            sources: vec![ContextSourceRef {
                source_id: "files:docs".to_string(),
                kind: "files".to_string(),
                content_hash: "sha256:333".to_string(),
            }],
            segment_hashes: HashMap::from([("stable".to_string(), "sha256:222".to_string())]),
        }),
        EventPayload::TaskRegistered(TaskRegisteredPayload {
            task_id: "graph-cmd".into(),
            criteria: vec![Criterion {
                cmd: "cargo test -p yunta".to_string(),
                r#type: None,
            }],
            scope: vec!["crates/cli/**".to_string()],
            depends_on: vec![],
        }),
        EventPayload::CriteriaChecked(CriteriaCheckedPayload {
            task_id: "graph-cmd".into(),
            phase: Phase::Pre,
            results: vec![CriterionResult {
                cmd: "cargo test -p yunta".to_string(),
                exit_code: 1,
                r#type: None,
                reused: false,
                duration_ms: None,
            }],
        }),
        EventPayload::TaskStatusChanged(TaskStatusChangedPayload {
            task_id: "graph-cmd".into(),
            new_status: TaskStatus::Done,
            caused_by: 42,
        }),
        EventPayload::ScopeChecked(ScopeCheckedPayload {
            task_id: Some("graph-cmd".into()),
            diff: vec!["crates/cli/src/graph.rs".into()],
            violations: vec![],
        }),
        EventPayload::ScopeExpansionRequested(ScopeExpansionRequestedPayload {
            task_id: "graph-cmd".into(),
            paths: vec!["crates/cli/src/**".to_string()],
            reason: "need to touch main.rs too".to_string(),
            proposed_criterion: None,
            proposed_criterion_precheck: None,
        }),
        EventPayload::ScopeExpansionGranted(ScopeExpansionGrantedPayload {
            task_id: "graph-cmd".into(),
            decided_by: Decider::Rule,
            mode: ScopeExpansionMode::Rules,
            count_this_run: 1,
            paths: vec!["crates/cli/src/**".to_string()],
        }),
        EventPayload::ScopeExpansionDenied(ScopeExpansionDeniedPayload {
            task_id: "graph-cmd".into(),
            decided_by: Decider::Person {
                id: "eulke".to_string(),
            },
            mode: ScopeExpansionMode::Ask,
            count_this_run: 2,
            denial_reason: Some("out of declared scope".to_string()),
        }),
        EventPayload::NodeFinished(NodeFinishedPayload {
            outcome: "criteria green".to_string(),
            tokens_used: TokenUsage {
                input: 10,
                output: 5,
                cached: None,
            },
        }),
        EventPayload::NodeFailed(NodeFailedPayload {
            outcome: "criteria red".to_string(),
            tokens_used: TokenUsage::default(),
            retryable: true,
        }),
        EventPayload::HookExecuted(HookExecutedPayload {
            phase: HookPhase::After,
            command: "cargo fmt".to_string(),
            exit_code: 0,
        }),
        EventPayload::NodeRerouted(NodeReroutedPayload {
            to_node: "fix-lint".into(),
            cause: "clippy failed".to_string(),
            attempt: 1,
            max_reroutes: 2,
        }),
        EventPayload::GateWaiting(GateWaitingPayload {
            summary: "Ready to open the PR?".to_string(),
            evidence: "all criteria green".to_string(),
            options: vec![GateOption {
                id: "approve".to_string(),
                label: "Approve and open the PR".to_string(),
                tradeoff: "opens the PR now".to_string(),
            }],
            external_ref: Some("https://github.com/example/repo/pull/1".to_string()),
        }),
        EventPayload::GateResolved(GateResolvedPayload {
            chosen_option: Some("approve".to_string()),
            resolved_by: Some("eulke".to_string()),
            free_text: None,
            approved_sha: Some("deadbeef".to_string()),
        }),
        EventPayload::QuestionsAnswered(QuestionsAnsweredPayload {
            answers_hash: "sha256:333".to_string(),
            channel: Channel::Tty,
            responder: Some("eulke".to_string()),
        }),
        EventPayload::LoopIteration(LoopIterationPayload {
            iteration: 3,
            until_result: false,
        }),
        EventPayload::FindingPosted(FindingPostedPayload {
            finding: Finding {
                id: "f-1".to_string(),
                severity: FindingSeverity::Major,
                title: "missing error handling".to_string(),
                location: "crates/cli/src/main.rs:10".to_string(),
                detail: "unwrap on a fallible call".to_string(),
                proposed_criterion: None,
            },
        }),
        EventPayload::PromotionSignaled(PromotionSignaledPayload {
            reason: "all quick-mode nodes green".to_string(),
            evidence: "criteria log".to_string(),
            suggested_mode: "standard".to_string(),
        }),
        EventPayload::ChildRunCreated(ChildRunCreatedPayload {
            child_run_id: "run-child-1".into(),
            child_workflow_hash: "sha256:444".to_string(),
        }),
        EventPayload::ChildRunFinished(ChildRunFinishedPayload {
            child_run_id: "run-child-1".into(),
            child_workflow_hash: "sha256:444".to_string(),
            terminal_state: TerminalState::Done,
            tokens: TokenUsage::default(),
        }),
        EventPayload::CapabilityDegraded(CapabilityDegradedPayload {
            capability: "resume_session".to_string(),
            adapter: "mock".to_string(),
            policy_applied: "on_interrupt: resume_session degraded to restart_node".to_string(),
        }),
        EventPayload::RunPaused(RunPausedPayload {
            reason: "gate waiting".to_string(),
        }),
        EventPayload::RunResumed(RunResumedPayload {
            resume_policy_applied: Some("restart_node".to_string()),
        }),
        EventPayload::RunFinished(RunFinishedPayload {
            terminal_state: TerminalState::Done,
            metrics: RunMetrics {
                cptv: Some(0.42),
                tokens: TokenUsage {
                    input: 1000,
                    output: 500,
                    cached: Some(200),
                },
            },
        }),
    ]
}

#[test]
fn there_are_exactly_31_kinds_with_distinct_names() {
    let kinds = all_kinds();
    assert_eq!(kinds.len(), 31);

    let names: std::collections::HashSet<&str> = kinds.iter().map(|k| k.kind_name()).collect();
    assert_eq!(names.len(), 31, "expected 31 distinct kind names");
}

#[test]
fn every_kind_round_trips_through_json() {
    for payload in all_kinds() {
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: EventPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, parsed, "round-trip mismatch for {json}");
    }
}

#[test]
fn every_kind_starts_at_schema_version_1() {
    for payload in all_kinds() {
        assert_eq!(payload.schema_version(), 1);
    }
}

#[test]
fn kind_names_match_the_spec_exactly() {
    let expected = [
        "run_created",
        "runner_resolved",
        "baseline_captured",
        "node_started",
        "agent_session_opened",
        "agent_message",
        "artifact_written",
        "context_assembled",
        "task_registered",
        "criteria_checked",
        "task_status_changed",
        "scope_checked",
        "scope_expansion_requested",
        "scope_expansion_granted",
        "scope_expansion_denied",
        "node_finished",
        "node_failed",
        "hook_executed",
        "node_rerouted",
        "gate_waiting",
        "gate_resolved",
        "questions_answered",
        "loop_iteration",
        "finding_posted",
        "promotion_signaled",
        "child_run_created",
        "child_run_finished",
        "capability_degraded",
        "run_paused",
        "run_resumed",
        "run_finished",
    ];
    let actual: Vec<&str> = all_kinds().iter().map(|k| k.kind_name()).collect();
    assert_eq!(actual, expected.to_vec());
}

#[test]
fn the_envelope_flattens_kind_and_payload_fields_together() {
    let event = Event {
        run_id: RunId::from("run-1"),
        seq: 1,
        timestamp: chrono::DateTime::parse_from_rfc3339("2026-08-18T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        node_id: None,
        payload: EventPayload::RunPaused(RunPausedPayload {
            reason: "gate waiting".to_string(),
        }),
    };

    let json: serde_json::Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["run_id"], "run-1");
    assert_eq!(json["seq"], 1);
    assert_eq!(json["kind"], "run_paused");
    assert_eq!(json["reason"], "gate waiting");
    assert!(json.get("node_id").is_none());

    let round_tripped: Event = serde_json::from_value(json).unwrap();
    assert_eq!(event, round_tripped);
}

#[test]
fn finding_severity_uses_the_contrato_wire_values() {
    for (severity, expected) in [
        (FindingSeverity::Blocking, "\"blocking\""),
        (FindingSeverity::Major, "\"major\""),
        (FindingSeverity::Minor, "\"minor\""),
        (FindingSeverity::Note, "\"note\""),
    ] {
        assert_eq!(serde_json::to_string(&severity).unwrap(), expected);
    }
}
