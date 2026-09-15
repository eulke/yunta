//! The event wire: every kind the log can carry, field by field.
//!
//! One sample of each kind is built through its constructor, so a
//! variant added without one fails to compile; the set of names, their
//! JSON round trip, their schema version and the domain each belongs to
//! are all asserted against that sample. A kind this binary does not
//! know reads as unknown and writes back verbatim, which is what lets an
//! older reader run against a newer log.

use yunta_core::events::EventShapeError;
use yunta_core::events::*;
use yunta_core::{Capability, NonEmpty, RunId};
use yunta_testkit_core::all_kinds;

/// A compile-time guard for [`all_kinds`], not a runtime check. The match has
/// one arm per `EventPayload` variant and no wildcard, so adding a variant to
/// the enum fails this crate's compile until the variant is also constructed
/// in `all_kinds` above. Without it, a new kind would be silently absent from
/// every round-trip, version, and name test in this file.
#[allow(dead_code)]
fn every_variant_is_built_by_all_kinds(payload: &EventPayload) {
    match payload {
        EventPayload::Run(RunEvent::Created(_))
        | EventPayload::Node(NodeEvent::RunnerResolved(_))
        | EventPayload::Run(RunEvent::BaselineCaptured(_))
        | EventPayload::Node(NodeEvent::Started(_))
        | EventPayload::Session(SessionEvent::Opened(_))
        | EventPayload::Session(SessionEvent::Message(_))
        | EventPayload::Artifacts(ArtifactEvent::Written(_))
        | EventPayload::Node(NodeEvent::ContextAssembled(_))
        | EventPayload::Tasks(TaskEvent::Registered(_))
        | EventPayload::Node(NodeEvent::CriteriaChecked(_))
        | EventPayload::Tasks(TaskEvent::StatusChanged(_))
        | EventPayload::Node(NodeEvent::ScopeChecked(_))
        | EventPayload::Scope(ScopeEvent::Requested(_))
        | EventPayload::Scope(ScopeEvent::Granted(_))
        | EventPayload::Scope(ScopeEvent::Denied(_))
        | EventPayload::Node(NodeEvent::Finished(_))
        | EventPayload::Node(NodeEvent::Failed(_))
        | EventPayload::Node(NodeEvent::HookExecuted(_))
        | EventPayload::Node(NodeEvent::Rerouted(_))
        | EventPayload::Gates(GateEvent::Waiting(_))
        | EventPayload::Gates(GateEvent::Resolved(_))
        | EventPayload::Gates(GateEvent::QuestionsAsked(_))
        | EventPayload::Gates(GateEvent::QuestionsAnswered(_))
        | EventPayload::Children(ChildEvent::LoopIteration(_))
        | EventPayload::Findings(FindingEvent::Posted(_))
        | EventPayload::Findings(FindingEvent::Updated(_))
        | EventPayload::Findings(FindingEvent::Withdrawn(_))
        | EventPayload::Findings(FindingEvent::Refused(_))
        | EventPayload::Artifacts(ArtifactEvent::Submitted(_))
        | EventPayload::Artifacts(ArtifactEvent::Accepted(_))
        | EventPayload::Run(RunEvent::PromotionSignaled(_))
        | EventPayload::Children(ChildEvent::Created(_))
        | EventPayload::Children(ChildEvent::Finished(_))
        | EventPayload::Session(SessionEvent::CapabilityDegraded(_))
        | EventPayload::Session(SessionEvent::WriteRefused(_))
        | EventPayload::Run(RunEvent::Paused(_))
        | EventPayload::Run(RunEvent::Resumed(_))
        | EventPayload::Run(RunEvent::Finished(_)) => {}
    }
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
                label: "Abort the run".to_string(),
                tradeoff: "Pauses here; nothing further executes".to_string(),
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
        refused.to_string(),
        "the summary repeats what the evidence already states: `exit 1`"
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
fn every_domain_declares_the_kinds_the_wire_carries() {
    let domains: Vec<(&str, &[&str])> = vec![
        ("run", RunEvent::KINDS),
        ("node", NodeEvent::KINDS),
        ("session", SessionEvent::KINDS),
        ("tasks", TaskEvent::KINDS),
        ("scope", ScopeEvent::KINDS),
        ("findings", FindingEvent::KINDS),
        ("artifacts", ArtifactEvent::KINDS),
        ("gates", GateEvent::KINDS),
        ("children", ChildEvent::KINDS),
    ];

    // Every kind belongs to exactly one domain, and between them they
    // account for the whole log: a kind in no domain is one nothing owns,
    // and a kind in two is a kind with two homes.
    let mut owned: Vec<&str> = domains
        .iter()
        .flat_map(|(_, kinds)| kinds.iter().copied())
        .collect();
    owned.sort_unstable();
    let mut declared: Vec<&str> = EventPayload::KINDS.to_vec();
    declared.sort_unstable();
    assert_eq!(
        owned, declared,
        "the domains and the wire name the same set of kinds"
    );

    // And each domain lists its own in the order the wire writes them, so
    // a reader moving between a domain and the log never re-sorts.
    for (name, kinds) in &domains {
        let placed: Vec<usize> = kinds
            .iter()
            .map(|kind| {
                EventPayload::KINDS
                    .iter()
                    .position(|k| k == kind)
                    .unwrap_or_else(|| {
                        panic!("`{kind}` of domain `{name}` is not a kind the wire carries")
                    })
            })
            .collect();
        let mut sorted = placed.clone();
        sorted.sort_unstable();
        assert_eq!(
            placed, sorted,
            "domain `{name}` lists its kinds in the order the wire writes them"
        );
    }
}

#[test]
fn there_are_exactly_38_kinds_with_distinct_names() {
    let kinds = all_kinds();
    assert_eq!(kinds.len(), 38);

    let names: std::collections::HashSet<&str> = kinds.iter().map(|k| k.kind_name()).collect();
    assert_eq!(names.len(), 38, "expected 38 distinct kind names");
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
        "questions_asked",
        "questions_answered",
        "loop_iteration",
        "finding_posted",
        "finding_updated",
        "finding_withdrawn",
        "finding_refused",
        "artifact_submitted",
        "artifact_accepted",
        "promotion_signaled",
        "child_run_created",
        "child_run_finished",
        "capability_degraded",
        "write_refused",
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
        body: EventBody::Known(EventPayload::Run(RunEvent::Paused(
            RunPausedPayload::recorded("gate waiting".to_string()),
        ))),
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
            assert_eq!(p.reason(), "waiting on gate approve")
        }
        other => panic!("expected run_paused, got {other:?}"),
    }
    assert_eq!(event.body.kind_name(), "run_paused");
}

#[test]
fn a_draft_names_what_happened_and_nothing_storage_assigns() {
    let draft = EventDraft {
        run_id: RunId::from("run-1"),
        node_id: None,
        payload: EventPayload::Run(RunEvent::Paused(RunPausedPayload::recorded(
            "budget".to_string(),
        ))),
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
    assert!(yunta_core::events::SessionEvent::KINDS.contains(&"write_refused"));

    let refused = yunta_core::events::WriteRefusedPayload::new(
        "sess-1".into(),
        yunta_core::events::ToolTarget::of_path(std::path::Path::new("docs/readme.md")),
    );
    let event = yunta_core::events::SessionEvent::WriteRefused(refused);
    assert!(
        !event.is_audit(),
        "a refusal is state a reader derives, not a note beside it"
    );
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
    assert!(RunEvent::KINDS.contains(&"baseline_captured"));
    assert!(
        !NodeEvent::KINDS.contains(&"baseline_captured"),
        "one domain owns a kind"
    );

    let event = RunEvent::BaselineCaptured(BaselineCapturedPayload {
        command: "cargo test".to_string(),
        results: BaselineResults {
            exit_code: 0,
            summary: "ok".to_string(),
        },
        hash: yunta_core::sha256_hex(b"ok"),
        origin: BaselineOrigin::Measured,
    });
    assert_eq!(event.kind_name(), "baseline_captured");
    assert!(
        !event.is_audit(),
        "what a run holds is derived from this, so a ledger folds it"
    );
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
