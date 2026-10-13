//! The event wire: every kind the log can carry, field by field.
//!
//! Each kind's sample comes from the registry, and the set of names, the
//! JSON round trip and the schema version are all asserted against those
//! samples. A kind this binary does not know reads as unknown and writes
//! back verbatim, which is what lets an older reader run against a newer
//! log.

use std::collections::BTreeSet;

use yunta_core::events::EventShapeError;
use yunta_core::events::*;
use yunta_core::{
    AdapterId, Capability, ConfigKey, NonEmpty, QuestionId, Redactor, RunId, ScopeExpansionMode,
    ScopeGlob, Secret, TaskId,
};
use yunta_testkit_core::all_kinds;

#[path = "events/failure.rs"]
mod failure;

#[test]
fn every_wire_kind_is_sampled() {
    let sampled: BTreeSet<&str> = all_kinds().iter().map(EventPayload::kind_name).collect();
    let unsampled: Vec<&str> = EventPayload::KINDS
        .iter()
        .copied()
        .filter(|k| !sampled.contains(k))
        .collect();
    assert!(
        unsampled.is_empty(),
        "kinds no sample builds: {unsampled:?}"
    );
}

#[test]
fn an_escalation_with_no_options_cannot_be_built() {
    // Not a runtime check: `NonEmpty::new` is the only way to a menu,
    // and it answers `None` for an empty one, so the escalation that
    // would refuse every answer given to it never exists.
    assert!(NonEmpty::new(Vec::<GateOption>::new()).is_none());
}

#[test]
fn an_escalation_whose_summary_repeats_a_fact_is_refused() {
    let menu = || {
        NonEmpty::from((
            GateOption {
                id: "abort".into(),
                label: Some("Abort the run".to_string()),
                tradeoff: Some("Pauses here; nothing further executes".to_string()),
                asks: None,
            },
            Vec::new(),
        ))
    };

    // A fact that names itself, repeated word for word in the claim:
    // every surface prints the two under separate headings, so this
    // reads as the same sentence twice.
    let refused = Escalation::new(
        "node `lint` failed with exit 1",
        vec![Fact::bare("exit 1")].into(),
        menu(),
    )
    .expect_err("the claim states the record");
    assert_eq!(
        refused,
        EscalationError::SummaryRepeatsEvidence {
            fact: Fact::bare("exit 1")
        }
    );
    assert_eq!(
        refused.to_string(),
        "the summary repeats what the evidence already states"
    );

    // A labelled fact is repeated when both halves are: the label is
    // what makes a bare number mean anything.
    assert!(Escalation::new(
        "run `r1` spent past its limits.max_tokens_per_run of 400",
        vec![Fact::labelled("limits.max_tokens_per_run", "400")].into(),
        menu(),
    )
    .is_err());

    // The same number without its label is a number the claim needed
    // for its own reasons, and no repetition of the record.
    assert!(Escalation::new(
        "node `lint` failed on attempt 400",
        vec![Fact::labelled("limits.max_tokens_per_run", "400")].into(),
        menu(),
    )
    .is_ok());
}

#[test]
fn a_done_task_carries_its_commit_and_nothing_else_does() {
    let commit = yunta_core::CommitSha::from("deadbeef");
    let done = TaskStatusChangedPayload::done("T001".into(), 1.into(), commit.clone());
    assert_eq!(done.new_status, TaskStatus::Done);
    assert_eq!(done.commit, Some(commit));

    // Every other status goes through `to`, which has nowhere to put a
    // commit: what a task that has not finished would be pointing at is
    // a question the type never asks.
    for status in [
        TaskStatus::Pending,
        TaskStatus::Ready,
        TaskStatus::Running,
        TaskStatus::Blocked,
        TaskStatus::Failed,
    ] {
        let changed = TaskStatusChangedPayload::to("T001".into(), status, 1.into());
        assert_eq!(changed.commit, None, "{status:?} names no commit");
    }
}

#[test]
fn there_are_exactly_59_kinds_with_distinct_names() {
    let kinds = all_kinds();
    assert_eq!(kinds.len(), 59);

    let names: std::collections::HashSet<&str> = kinds.iter().map(|k| k.kind_name()).collect();
    assert_eq!(names.len(), 59, "expected 59 distinct kind names");
}

/// A node that asked nothing did not ask: the fact refuses to exist, so
/// no log can hold a wait nobody can end.
#[test]
fn a_questions_asked_with_no_questions_cannot_be_built() {
    assert!(QuestionsAskedPayload::new(
        yunta_core::sha256_hex(b"empty"),
        Vec::new(),
        TokenUsage::default(),
    )
    .is_none());
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
    assert_eq!(KIND_VERSION, 1);
    for payload in all_kinds() {
        assert_eq!(EventBody::Known(payload).schema_version(), KIND_VERSION);
    }
}

#[test]
fn a_payload_without_a_secret_passes_the_redactor_unchanged() {
    let redactor = Redactor::of_values([Secret::new("a-value-no-payload-carries".to_string())]);
    for payload in all_kinds() {
        let event = redactor
            .payload(payload.clone())
            .expect("no secret is in the payload, so nothing is refused");
        assert_eq!(event, payload);
    }
}

/// Every kind's name, in the order the binary writes them.
const KIND_NAMES: &[&str] = &[
    "run_created",
    "runner_resolved",
    "baseline_captured",
    "baseline_held",
    "node_started",
    "agent_session_opened",
    "agent_message",
    "artifact_written",
    "context_assembled",
    "task_registered",
    "criteria_checked",
    "task_status_changed",
    "task_check_started",
    "task_check_answered",
    "deviation_declared",
    "deviation_resolved",
    "criterion_declared_unpassable",
    "scope_checked",
    "scope_expansion_requested",
    "scope_expansion_granted",
    "scope_expansion_denied",
    "scope_derived",
    "scope_expansion_withdrawn",
    "node_finished",
    "node_failed",
    "hook_executed",
    "node_rerouted",
    "pull_request_opened",
    "gate_waiting",
    "gate_resolved",
    "questions_asked",
    "questions_answered",
    "asking_opened",
    "loop_iteration",
    "finding_posted",
    "finding_updated",
    "finding_withdrawn",
    "finding_refused",
    "finding_answered",
    "finding_proved",
    "finding_settled",
    "artifact_submitted",
    "artifact_accepted",
    "promotion_signaled",
    "child_run_created",
    "child_run_finished",
    "capability_degraded",
    "write_refused",
    "stop_refused",
    "write_formatted",
    "run_tool_failed",
    "run_tool_refused",
    "service_unreachable",
    "service_reachable",
    "host_suspended",
    "run_paused",
    "run_resumed",
    "run_moved",
    "run_finished",
];

#[test]
fn kind_names_match_the_spec_exactly() {
    let expected = KIND_NAMES;
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
        body: EventBody::Known(EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
            &PauseReason::Recorded {
                sentence: "gate waiting".to_string(),
            },
        )))),
    };

    let json: serde_json::Value = serde_json::to_value(&event).unwrap();
    assert_eq!(json["run_id"], "run-1");
    assert_eq!(json["seq"], 1);
    assert_eq!(json["kind"], "run_paused");
    assert_eq!(
        json["pause"],
        serde_json::json!({"type": "recorded", "sentence": "gate waiting"})
    );
    assert!(json.get("node_id").is_none());

    let round_tripped: StoredEvent = serde_json::from_value(json).unwrap();
    assert_eq!(event, round_tripped);
}

#[test]
fn finding_severity_serializes_to_its_wire_values() {
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
fn a_gate_resolution_is_the_shape_its_fields_spell() {
    // Four shapes, one flat wire object: the fields present say which.
    let shapes: [(GateResolvedPayload, &[&str]); 4] = [
        (
            GateResolvedPayload::Chosen(HumanChoice {
                option: "retry".into(),
                by: "eulke".into(),
                free_text: Some("one more lap".to_string()),
            }),
            &["chosen_option", "resolved_by", "free_text"],
        ),
        (
            GateResolvedPayload::Approved {
                by: "reviewer".into(),
                sha: "deadbeef".into(),
            },
            &["resolved_by", "approved_sha"],
        ),
        (
            GateResolvedPayload::ChangesRequested {
                by: "reviewer".into(),
            },
            &["resolved_by"],
        ),
        (GateResolvedPayload::Closed, &[]),
    ];
    for (shape, fields) in shapes {
        let json =
            serde_json::to_value(EventPayload::Gates(GateEvent::Resolved(shape.clone()))).unwrap();
        let mut present: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|key| *key != "kind")
            .collect();
        present.sort_unstable();
        let mut expected = fields.to_vec();
        expected.sort_unstable();
        assert_eq!(present, expected, "the wire for {shape:?}");
        let parsed: EventPayload = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, EventPayload::Gates(GateEvent::Resolved(shape)));
    }
}

#[test]
fn an_unnamed_gate_resolution_reads_as_unrecognized_and_writes_back_verbatim() {
    // A choice and an approval SHA in the same record: no shape this
    // binary names. It is neither refused nor reinterpreted — the export
    // writes it back exactly as it came.
    let json = serde_json::json!({
        "kind": "gate_resolved",
        "chosen_option": "approve",
        "resolved_by": "eulke",
        "approved_sha": "deadbeef"
    });
    let parsed: EventPayload = serde_json::from_value(json.clone()).unwrap();
    assert!(
        matches!(
            parsed,
            EventPayload::Gates(GateEvent::Resolved(GateResolvedPayload::Unrecognized(_)))
        ),
        "got {parsed:?}"
    );
    assert_eq!(serde_json::to_value(&parsed).unwrap(), json);
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
        Some(EventPayload::Run(RunEvent::Paused(p))) => {
            assert_eq!(
                p.pause,
                PauseReason::Recorded {
                    sentence: "waiting on gate approve".to_string()
                }
            )
        }
        other => panic!("expected run_paused, got {other:?}"),
    }
    assert_eq!(event.body.kind_name(), "run_paused");
}

#[test]
fn a_draft_names_what_happened_and_nothing_storage_assigns() {
    let draft = EventDraft::new(
        RunId::from("run-1"),
        None,
        EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
            &PauseReason::Recorded {
                sentence: "budget".to_string(),
            },
        ))),
    )
    .expect("a draft names its subject");
    assert_eq!(draft.payload().kind_name(), "run_paused");
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
    // `capability_degraded.capability` names a field of `Capabilities`;
    // a name no adapter can declare is a payload that does not fit its
    // kind, reported like any other misshapen body.
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
    let EventBody::Known(EventPayload::Session(SessionEvent::CapabilityDegraded(payload))) = body
    else {
        panic!("a capability_degraded body parses as its payload: {body:?}");
    };
    assert_eq!(payload.capability, Capability::NetworkIsolation);
}

/// A refusal moves the session it belongs to, so a reader can say what
/// a node's sessions were stopped from writing.
#[test]
fn write_refused_is_a_session_kind_that_moves_the_ledger() {
    assert!(EventPayload::KINDS.contains(&"write_refused"));

    let refused = yunta_core::events::WriteRefusedPayload::new(
        "sess-1".into(),
        yunta_core::events::ToolTarget::of_path(std::path::Path::new("docs/readme.md")),
    );
    let event = EventPayload::Session(yunta_core::events::SessionEvent::WriteRefused(refused));
    assert_eq!(event.kind_name(), "write_refused");
}

/// A log written before the fence existed carries `edit_hooks`, and a
/// reader takes that as an adapter that built no fence at all.
#[test]
fn an_old_log_without_a_fence_level_reads_as_none() {
    let old = serde_json::json!({
        "resume_session": true,
        "edit_hooks": true,
        "permission_profiles": true,
        "custom_agents": false,
        "usage_reporting": true,
        "run_tools": false,
    });
    let read: yunta_core::Capabilities = serde_json::from_value(old).unwrap();

    assert_eq!(read.fence, yunta_core::FenceLevel::None);
    assert!(read.resume_session, "what it did say still reads");
}

/// The measurement is the run's, not a node's: `RunLedger` folds it, so
/// it is state a reader derives rather than a note beside it.
#[test]
fn baseline_captured_is_a_run_kind_that_moves_the_ledger() {
    assert!(EventPayload::KINDS.contains(&"baseline_captured"));

    let event = EventPayload::Run(RunEvent::BaselineCaptured(BaselineCapturedPayload {
        command: "cargo test".to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: "ok".to_string(),
        },
        hash: yunta_core::sha256_hex(b"ok"),
        origin: BaselineOrigin::Measured,
        tree: None,
        duration_ms: None,
    }));
    assert_eq!(event.kind_name(), "baseline_captured");
}

/// A log written before a lineage could hand a measurement down carries
/// no `origin`, and every measurement on it is one the run took itself.
#[test]
fn a_baseline_captured_written_without_an_origin_reads_as_measured() {
    let old = serde_json::json!({
        "command": "cargo test",
        "results": { "exit_code": 0, "summary": "ok" },
        "hash": yunta_core::sha256_hex(b"ok").as_str(),
    });
    let read: BaselineCapturedPayload = serde_json::from_value(old).unwrap();

    assert_eq!(read.origin, BaselineOrigin::Measured);
    assert_eq!(read.command, "cargo test");
}

/// What a node waits on is a fact about the node, and the two waits are
/// not the same fact: a gate has a handle, a node that asked has the
/// questions nobody answered.
#[test]
fn a_node_that_asked_waits_on_its_questions_in_its_own_state() {
    let asked = NonEmpty::new(vec![QuestionId::from("q-scope"), QuestionId::from("q-api")])
        .expect("a node that asked, asked something");
    let waiting = NodeState::Waiting {
        on: NodeWait::Questions {
            asked: asked.clone(),
        },
    };
    let Some(NodeWait::Questions { asked: held }) = waiting.waiting_on() else {
        panic!("a node that asked says what it asked: {waiting:?}");
    };
    assert_eq!(held, &asked);

    let gate = NodeState::Waiting {
        on: NodeWait::Gate {
            external_ref: Some("https://forge/pr/1".to_string()),
        },
    };
    assert_ne!(
        gate, waiting,
        "a published gate and an unanswered question are two different waits"
    );
}

/// The one sentence every surface says a node's unanswered questions
/// with — the node's own label, the run's pause, the chronicle.
#[test]
fn questions_awaiting_an_answer_are_said_one_way() {
    assert_eq!(
        yunta_core::text::asked_questions(&[QuestionId::from("q-scope")]),
        "asked 1 question: `q-scope`"
    );
    assert_eq!(
        yunta_core::text::asked_questions(&[
            QuestionId::from("q-scope"),
            QuestionId::from("q-api")
        ]),
        "asked 2 questions: `q-scope`, `q-api`"
    );
}

#[test]
fn a_session_end_this_build_does_not_know_reads_back_as_unknown() {
    let end: SessionEnd =
        serde_json::from_value(serde_json::json!({"type": "stopped", "by": "a debugger"})).unwrap();
    assert_eq!(end, SessionEnd::Unknown);
}

#[test]
fn a_node_started_without_a_tree_reads_as_the_runs_own_base() {
    // A log written before a start named the tree it began from: the
    // field is absent, and a reader takes the run's own base, which is
    // what that log always meant.
    let payload: NodeStartedPayload =
        serde_json::from_value(serde_json::json!({"attempt": 1})).unwrap();
    assert_eq!(payload.attempt, 1);
    assert_eq!(payload.from_tree, None);

    // And a start that named one round-trips carrying it.
    let named = NodeStartedPayload::attempt_from(2, "a1b2c3d4e5f6".parse().unwrap());
    let json = serde_json::to_value(&named).unwrap();
    assert_eq!(json["from_tree"], "a1b2c3d4e5f6");
    assert_eq!(
        serde_json::from_value::<NodeStartedPayload>(json).unwrap(),
        named
    );
}

/// A log written before a finish named the tree the node left reads as
/// a finish that says nothing about the tree; one that names it
/// round-trips carrying it.
#[test]
fn a_node_finished_without_a_tree_reads_as_naming_none() {
    let old: NodeFinishedPayload = serde_json::from_value(serde_json::json!({
        "outcome": "exit 0",
        "tokens_used": {"input": 0, "output": 0}
    }))
    .unwrap();
    assert_eq!(old.tree, None);

    let named = NodeFinishedPayload::leaving(
        NodeOutcome::Summary("exit 0".to_string()),
        TokenUsage::default(),
        "a1b2c3d4e5f6".parse().unwrap(),
    );
    let json = serde_json::to_value(&named).unwrap();
    assert_eq!(json["tree"], "a1b2c3d4e5f6");
    assert_eq!(
        serde_json::from_value::<NodeFinishedPayload>(json).unwrap(),
        named
    );
}

/// A log written before sessions named their task and the session they
/// continue, and before a reopening named the session it resumes, reads
/// as sessions and reopenings that name neither.
#[test]
fn a_log_written_before_continuations_reads_as_naming_none() {
    let opened: EventPayload = serde_json::from_value(serde_json::json!({
        "kind": "agent_session_opened",
        "session_id": "s-1",
        "capabilities": {}
    }))
    .unwrap();
    let EventPayload::Session(yunta_core::events::SessionEvent::Opened(opened)) = opened else {
        panic!("reads as the session it is: {opened:?}");
    };
    assert_eq!((opened.task_id, opened.continues), (None, None));

    let reopened: EventPayload = serde_json::from_value(serde_json::json!({
        "kind": "task_status_changed",
        "task_id": "T001",
        "new_status": "pending",
        "caused_by": 7,
        "left_work": "0a753387f5ecb38915805fad12cc4ddba873b0d6"
    }))
    .unwrap();
    let EventPayload::Tasks(yunta_core::events::TaskEvent::StatusChanged(reopened)) = reopened
    else {
        panic!("reads as the status change it is: {reopened:?}");
    };
    assert_eq!(reopened.resumes, None);
}

/// A pause is stored as the fact that stopped the run, not as the
/// sentence a reader will see.
#[test]
fn a_pause_is_data_not_a_sentence() {
    let pause = PauseReason::BudgetExhausted { spent: 5, cap: 4 };
    let json = serde_json::to_value(EventPayload::Run(RunEvent::Paused(RunPausedPayload::new(
        &pause,
    ))))
    .unwrap();
    assert!(json.get("reason").is_none(), "{json}");
    assert_eq!(json["pause"]["type"], "budget_exhausted", "{json}");
}

/// A denial a node received names no task, and reads back without one.
#[test]
fn a_node_scope_denial_reads_back_without_a_task() {
    let line = serde_json::json!({
        "kind": "scope_expansion_denied",
        "decided_by": {"type": "rule"},
        "mode": "ask",
        "count_this_run": 1
    });
    let payload: EventPayload = serde_json::from_value(line).expect("a node's denial parses");
    assert!(matches!(
        payload,
        EventPayload::Scope(ScopeEvent::Denied(_))
    ));
    let written = serde_json::to_value(&payload).unwrap();
    assert!(written.get("task_id").is_none(), "{written}");
}

/// A grant that names neither a task nor a node widens nothing, so the
/// ledger does not count it.
#[test]
fn a_grant_naming_no_subject_counts_for_nothing() {
    let grant = ScopeEvent::Granted(ScopeExpansionGrantedPayload {
        task_id: None,
        decided_by: Decider::Rule,
        mode: Some(ScopeExpansionMode::Rules),
        count_this_run: 1,
        paths: vec![],
    });
    let meta = EventMeta {
        seq: 1.into(),
        at: chrono::DateTime::UNIX_EPOCH,
        node: None,
    };
    let mut ledger = GrantLedger::default();
    let _ = ledger.apply(&grant, &meta);
    assert_eq!(ledger.granted(), 0);
}
