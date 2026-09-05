use std::collections::BTreeMap;

use yunta_core::events::EventShapeError;
use yunta_core::events::*;
use yunta_core::ScopeExpansionMode;
use yunta_core::{Capability, RunId, RunnerCandidate};

fn all_kinds() -> Vec<EventPayload> {
    vec![
        EventPayload::RunCreated(RunCreatedPayload {
            manifest_hash: yunta_core::sha256_hex(b"sha256:abc"),
            inputs: BTreeMap::new(),
            mode: "default".into(),
            promoted_from: None,
            yunta_schema: None,
            base_branch: "main".to_string(),
            base_commit: "deadbeef".into(),
        }),
        EventPayload::RunnerResolved(RunnerResolvedPayload {
            runner: "executor".into(),
            chosen: RunnerCandidate {
                adapter: "mock".into(),
                model: "mock-model".into(),
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
            hash: yunta_core::sha256_hex(b"sha256:def"),
        }),
        EventPayload::NodeStarted(NodeStartedPayload { attempt: 1 }),
        EventPayload::AgentSessionOpened(AgentSessionOpenedPayload {
            session_id: "sess-1".into(),
            agent: None,
            model: Some("mock-model".into()),
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
            content_hash: yunta_core::sha256_hex(b"sha256:111"),
            artifact_kind: Some(yunta_core::ArtifactKind::TaskLedger),
        }),
        EventPayload::ContextAssembled(ContextAssembledPayload {
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
            caused_by: 42.into(),
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
            attempt: Some(1),
            max_reroutes: Some(2),
            origin: yunta_core::events::RerouteOrigin::OnFailure,
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
            approved_sha: Some("deadbeef".into()),
        }),
        EventPayload::QuestionsAnswered(QuestionsAnsweredPayload {
            answers_hash: yunta_core::sha256_hex(b"sha256:333"),
            channel: Channel::Tty,
            responder: Some("eulke".to_string()),
        }),
        EventPayload::LoopIteration(LoopIterationPayload {
            iteration: 3,
            until_result: false,
        }),
        EventPayload::FindingPosted(FindingPostedPayload {
            finding: Finding {
                id: "f-1".into(),
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
            suggested_mode: "standard".into(),
        }),
        EventPayload::ChildRunCreated(ChildRunCreatedPayload {
            child_run_id: "run-child-1".into(),
            child_workflow_hash: yunta_core::sha256_hex(b"sha256:444"),
        }),
        EventPayload::ChildRunFinished(ChildRunFinishedPayload {
            child_run_id: "run-child-1".into(),
            child_workflow_hash: yunta_core::sha256_hex(b"sha256:444"),
            terminal_state: TerminalState::Done,
            tokens: TokenUsage::default(),
        }),
        EventPayload::CapabilityDegraded(CapabilityDegradedPayload {
            capability: Capability::ResumeSession,
            adapter: "mock".into(),
            policy_applied: "on_interrupt: resume_session degraded to restart_node".to_string(),
        }),
        EventPayload::RunPaused(RunPausedPayload {
            reason: "gate waiting".to_string(),
        }),
        EventPayload::RunResumed(RunResumedPayload {
            resume_policy_applied: Some("restart_node".to_string()),
            policies: Vec::new(),
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

/// A compile-time guard for [`all_kinds`], not a runtime check. The match has
/// one arm per `EventPayload` variant and no wildcard, so adding a variant to
/// the enum fails this crate's compile until the variant is also constructed
/// in `all_kinds` above. Without it, a new kind would be silently absent from
/// every round-trip, version, and name test in this file.
#[allow(dead_code)]
fn every_variant_is_built_by_all_kinds(payload: &EventPayload) {
    match payload {
        EventPayload::RunCreated(_)
        | EventPayload::RunnerResolved(_)
        | EventPayload::BaselineCaptured(_)
        | EventPayload::NodeStarted(_)
        | EventPayload::AgentSessionOpened(_)
        | EventPayload::AgentMessage(_)
        | EventPayload::ArtifactWritten(_)
        | EventPayload::ContextAssembled(_)
        | EventPayload::TaskRegistered(_)
        | EventPayload::CriteriaChecked(_)
        | EventPayload::TaskStatusChanged(_)
        | EventPayload::ScopeChecked(_)
        | EventPayload::ScopeExpansionRequested(_)
        | EventPayload::ScopeExpansionGranted(_)
        | EventPayload::ScopeExpansionDenied(_)
        | EventPayload::NodeFinished(_)
        | EventPayload::NodeFailed(_)
        | EventPayload::HookExecuted(_)
        | EventPayload::NodeRerouted(_)
        | EventPayload::GateWaiting(_)
        | EventPayload::GateResolved(_)
        | EventPayload::QuestionsAnswered(_)
        | EventPayload::LoopIteration(_)
        | EventPayload::FindingPosted(_)
        | EventPayload::PromotionSignaled(_)
        | EventPayload::ChildRunCreated(_)
        | EventPayload::ChildRunFinished(_)
        | EventPayload::CapabilityDegraded(_)
        | EventPayload::RunPaused(_)
        | EventPayload::RunResumed(_)
        | EventPayload::RunFinished(_) => {}
    }
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
    let event = StoredEvent {
        run_id: RunId::from("run-1"),
        seq: 1.into(),
        timestamp: chrono::DateTime::parse_from_rfc3339("2026-08-18T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        node_id: None,
        body: EventBody::Known(EventPayload::RunPaused(RunPausedPayload {
            reason: "gate waiting".to_string(),
        })),
    };

    let json: serde_json::Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["run_id"], "run-1");
    assert_eq!(json["seq"], 1);
    assert_eq!(json["kind"], "run_paused");
    assert_eq!(json["reason"], "gate waiting");
    assert!(json.get("node_id").is_none());

    let round_tripped: StoredEvent = serde_json::from_value(json).unwrap();
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

#[test]
fn an_unknown_kind_reads_as_unknown_and_writes_back_verbatim() {
    let json = serde_json::json!({
        "run_id": "run-1",
        "seq": 4,
        "timestamp": "2026-09-02T10:00:00Z",
        "node_id": "plan",
        "kind": "future_kind",
        "novel": { "nested": [1, 2, 3] }
    });
    let event: StoredEvent = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(event.seq.get(), 4);
    assert_eq!(event.node_id.as_ref().map(|id| id.as_str()), Some("plan"));
    assert!(event.payload().is_none());
    let EventBody::Unknown(unknown) = &event.body else {
        panic!("expected an unknown body, got {:?}", event.body);
    };
    assert_eq!(unknown.kind, "future_kind");
    assert_eq!(event.body.kind_name(), "future_kind");
    assert_eq!(unknown.payload["novel"]["nested"][2], 3);

    // Written back: the body verbatim, plus the version the unknown kind
    // was stored under — the one envelope field an unknown body needs.
    let written = serde_json::to_value(&event).unwrap();
    assert_eq!(written["kind"], json["kind"]);
    assert_eq!(written["novel"], json["novel"]);
    assert_eq!(written["run_id"], json["run_id"]);
    assert_eq!(written["seq"], json["seq"]);
    assert_eq!(written["node_id"], json["node_id"]);
    assert_eq!(written["schema_version"], 1);
    let again: StoredEvent = serde_json::from_value(written).unwrap();
    assert_eq!(again, event);
}

#[test]
fn a_known_kind_reads_as_its_payload_and_ignores_fields_it_does_not_know() {
    let json = serde_json::json!({
        "run_id": "run-1",
        "seq": 1,
        "timestamp": "2026-09-02T10:00:00Z",
        "kind": "run_paused",
        "reason": "waiting on gate approve",
        "added_by_a_newer_binary": true
    });
    let event: StoredEvent = serde_json::from_value(json).unwrap();
    match event.payload() {
        Some(EventPayload::RunPaused(p)) => assert_eq!(p.reason, "waiting on gate approve"),
        other => panic!("expected run_paused, got {other:?}"),
    }
    assert_eq!(event.body.kind_name(), "run_paused");
}

#[test]
fn a_draft_names_what_happened_and_nothing_storage_assigns() {
    let draft = EventDraft {
        run_id: RunId::from("run-1"),
        node_id: None,
        payload: EventPayload::RunPaused(RunPausedPayload {
            reason: "budget".to_string(),
        }),
    };
    assert_eq!(draft.payload.kind_name(), "run_paused");
}

#[test]
fn a_body_without_a_kind_is_refused_naming_the_gap() {
    let error = EventBody::from_object(serde_json::Map::new(), 1).unwrap_err();
    assert!(matches!(error, EventShapeError::KindMissing), "{error:?}");
}

#[test]
fn a_known_kind_whose_fields_do_not_fit_names_the_kind_and_keeps_the_cause() {
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), serde_json::json!("run_paused"));
    let error = EventBody::from_object(object, 1).unwrap_err();
    assert!(
        matches!(&error, EventShapeError::Payload { kind, .. } if kind == "run_paused"),
        "{error:?}"
    );
    assert!(
        std::error::Error::source(&error).is_some(),
        "the serde cause travels with the error"
    );
}

#[test]
fn pr_is_not_a_questions_channel() {
    // No surface answers a `kind: questions` artifact over a PR, so the
    // channel enum carries only the two that do: tty and mcp.
    assert!(serde_json::from_str::<Channel>("\"tty\"").is_ok());
    assert!(serde_json::from_str::<Channel>("\"mcp\"").is_ok());
    assert!(
        serde_json::from_str::<Channel>("\"pr\"").is_err(),
        "`pr` is retired — no emitter produces it"
    );
}

#[test]
fn a_capability_outside_capabilities_fields_is_not_an_event() {
    // `capability_degraded.capability` names a field of `Capabilities`
    // (spec-events §5.24); a name no adapter can declare is a payload that
    // does not fit its kind, reported like any other misshapen body.
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), serde_json::json!("capability_degraded"));
    object.insert("capability".to_string(), serde_json::json!("teleport"));
    object.insert("adapter".to_string(), serde_json::json!("mock"));
    object.insert("policy_applied".to_string(), serde_json::json!("none"));
    let error = EventBody::from_object(object, 1).unwrap_err();
    assert!(
        matches!(&error, EventShapeError::Payload { kind, .. } if kind == "capability_degraded"),
        "{error:?}"
    );
}

#[test]
fn a_capability_parses_to_the_capabilities_field_it_names() {
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), serde_json::json!("capability_degraded"));
    object.insert(
        "capability".to_string(),
        serde_json::json!("network_isolation"),
    );
    object.insert("adapter".to_string(), serde_json::json!("mock"));
    object.insert(
        "policy_applied".to_string(),
        serde_json::json!("declarative-only"),
    );
    let body = EventBody::from_object(object, 1).unwrap();
    let EventBody::Known(EventPayload::CapabilityDegraded(payload)) = body else {
        panic!("a capability_degraded body parses as its payload: {body:?}");
    };
    assert_eq!(payload.capability, Capability::NetworkIsolation);
}
