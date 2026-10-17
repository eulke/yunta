//! A node's failure as the log holds it: every variant reads back as itself,
//! and each kind of failure keeps the facts its sentence names, a command's
//! origin and output, a session's exit, and the paths a scope violation
//! refused.

use super::failure_samples::*;
use super::*;

#[test]
fn a_node_failed_by_a_dead_session_round_trips_with_its_exit() {
    let payload = EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::session_died(
            "codex".parse().unwrap(),
            Some(SessionExit {
                end: SessionEnd::Code { code: 2 },
                stderr_tail: vec!["url is not supported for stdio".to_string()],
            }),
        ),
        TokenUsage::default(),
    )));

    let json: serde_json::Value = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["died"]["adapter"], "codex");
    assert_eq!(json["died"]["exit"]["end"]["type"], "code");
    assert_eq!(json["died"]["exit"]["end"]["code"], 2);

    let parsed: EventPayload = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(payload, parsed, "{json}");
}

/// A command that exited 101 having printed a compiler's error on
/// stdout, the way `cargo build` prints one.
fn compiler_exit(origin: Option<CommandOrigin>) -> Failure {
    Failure::exited(CommandExit {
        origin,
        code: 101,
        tail: vec![
            "error[E0425]: cannot find value `x` in this scope".to_string(),
            " --> src/lib.rs:3:5".to_string(),
        ],
        output: Some(yunta_core::sha256_hex(b"everything it printed")),
    })
}

#[test]
fn a_node_failed_by_a_command_round_trips_with_what_it_printed() {
    let payload = EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        compiler_exit(None),
        TokenUsage::default(),
    )));

    let json: serde_json::Value = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["exited"]["code"], 101);
    assert_eq!(json["exited"]["tail"][1], " --> src/lib.rs:3:5");
    assert!(json["exited"]["output"].is_string(), "{json}");
    assert!(
        json.get("outcome").is_none(),
        "the prose is produced from the data, never stored beside it: {json}"
    );

    let parsed: EventPayload = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(payload, parsed, "{json}");
}

#[test]
fn a_failure_logged_as_a_sentence_before_commands_were_data_still_reads_as_one() {
    let parsed: Failure =
        serde_json::from_value(serde_json::json!({ "outcome": "exit 1: no such file" })).unwrap();
    assert_eq!(
        parsed,
        Failure::Message {
            outcome: "exit 1: no such file".into(),
        }
    );
}

#[test]
fn an_older_log_sentence_for_an_engine_error_a_denial_or_a_stop_still_reads() {
    let engine: Failure = serde_json::from_value(serde_json::json!({
        "engine_error": "pushing `yunta/run/01` to `origin`: rejected",
    }))
    .expect("an engine error stored as a sentence reads back");
    assert_eq!(
        engine,
        Failure::EngineFailed {
            engine_error: EngineError::Logged(
                "pushing `yunta/run/01` to `origin`: rejected".into()
            ),
        }
    );

    let denial = "command `rm -rf x` matches denied pattern `rm *` (permissions.commands.deny)";
    let denied: Failure =
        serde_json::from_value(serde_json::json!({ "denied_rule": denial })).expect("reads back");
    assert_eq!(
        denied,
        Failure::CommandDenied {
            denied_rule: DeniedRule::Logged(denial.into()),
        }
    );

    let stopped: Failure = serde_json::from_value(serde_json::json!({
        "session_not_stopped": "kill failed; taking back what the session set up failed too: busy",
    }))
    .expect("a stop stored as one sentence reads back");
    assert_eq!(
        stopped,
        Failure::SessionNotStopped {
            session_not_stopped:
                "kill failed; taking back what the session set up failed too: busy".into(),
            release_failed: None,
        }
    );
    let written = serde_json::to_value(&stopped).unwrap();
    assert!(written.get("release_failed").is_none(), "{written}");
}

/// The command a failure is about. Only a command that exited non-zero
/// fails as `Exited`, and any other failure fails the test here.
fn exited_command(failure: Failure) -> CommandExit {
    let Failure::Exited { exited } = failure else {
        panic!("a command that exits non-zero fails as Exited");
    };
    exited
}

#[test]
fn a_failed_command_keeps_its_origin_its_code_and_its_last_lines() {
    let exited = exited_command(compiler_exit(None));
    assert_eq!(exited.origin, None);
    assert_eq!(exited.code, 101);
    assert_eq!(
        exited.tail,
        [
            "error[E0425]: cannot find value `x` in this scope",
            " --> src/lib.rs:3:5",
        ]
    );

    let exited = exited_command(compiler_exit(Some(CommandOrigin::Executor {
        executor: "probe".to_string(),
    })));
    assert_eq!(
        exited.origin,
        Some(CommandOrigin::Executor {
            executor: "probe".to_string(),
        })
    );
    assert_eq!(exited.code, 101);

    let exited = exited_command(compiler_exit(Some(CommandOrigin::Hook {
        phase: HookPhase::Before,
        command: "cargo fmt --check".to_string(),
    })));
    assert_eq!(
        exited.origin,
        Some(CommandOrigin::Hook {
            phase: HookPhase::Before,
            command: "cargo fmt --check".to_string(),
        })
    );

    let exited = exited_command(Failure::exited(CommandExit {
        origin: None,
        code: 1,
        tail: Vec::new(),
        output: None,
    }));
    assert!(
        exited.tail.is_empty(),
        "a command that printed nothing keeps no lines"
    );
    assert_eq!(exited.code, 1);
}

#[test]
fn a_check_refused_on_an_unchanged_tree_still_carries_what_the_attempt_that_ran_printed() {
    let refused = Failure::unchanged(1, compiler_exit(None));
    assert_eq!(refused.tail(), compiler_exit(None).tail());
    assert_eq!(refused.output(), compiler_exit(None).output());
    let Failure::Unchanged { unchanged } = &refused else {
        panic!("a refusal is an Unchanged failure, got {refused:?}");
    };
    assert_eq!(unchanged.since, 1);
    assert_eq!(*unchanged.failure, compiler_exit(None));
}

#[test]
fn a_node_failed_outside_its_scope_round_trips_naming_each_path() {
    let payload = EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        Failure::scope_violated(vec!["crates/cli/Cargo.toml".into(), "clippy.toml".into()]),
        TokenUsage::default(),
    )));

    let json: serde_json::Value = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        json["outside_scope"],
        serde_json::json!(["crates/cli/Cargo.toml", "clippy.toml"])
    );

    let parsed: EventPayload = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(payload, parsed, "{json}");
}

/// A check refused on an unchanged tree carries the failure of the
/// attempt that ran, and a refusal of a refusal still names that one.
#[test]
fn a_check_refused_on_an_unchanged_tree_round_trips_naming_the_attempt_that_ran() {
    let refused = Failure::unchanged(
        1,
        Failure::Message {
            outcome: "regression: exit 101".into(),
        },
    );
    let again = Failure::unchanged(2, refused.clone());
    assert_eq!(again, refused, "the second refusal names attempt 1 too");

    let payload = EventPayload::Node(NodeEvent::Failed(NodeFailedPayload::new(
        refused,
        TokenUsage::default(),
    )));
    let json: serde_json::Value = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["unchanged"]["since"], 1);
    assert_eq!(
        json["unchanged"]["failure"]["outcome"],
        "regression: exit 101"
    );
    let parsed: EventPayload = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(payload, parsed, "{json}");
}

/// Every failure variant, sampled, is written as JSON and reads back as itself.
#[test]
fn every_failure_variant_reads_back_as_itself() {
    let samples = plain_failure_samples()
        .into_iter()
        .chain(session_and_limit_samples())
        .chain(loop_check_and_child_samples())
        .chain(typed_cause_samples())
        .chain(artifact_and_template_samples())
        .chain(mount_samples())
        .chain(confinement_and_frozen_samples())
        .chain(composed_capability_and_skill_samples())
        .chain(engine_error_and_denial_samples())
        .chain(join_gate_and_review_samples())
        .chain(loop_and_runner_samples())
        .chain(refusal_samples());
    for failure in samples {
        let json = serde_json::to_value(&failure).unwrap();
        let parsed: Failure = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(parsed, failure, "{json}");
    }
}

#[test]
fn a_document_label_that_no_kind_names_is_refused_when_read() {
    let json = serde_json::json!({
        "workflow": "sub",
        "refusal": {
            "refusal": "inputs",
            "problem": {
                "kind": "document_unreadable",
                "name": "plan",
                "path": "plan.md",
                "label": "no kind",
                "detail": "is a directory",
            },
        },
    });
    assert!(serde_json::from_value::<Failure>(json).is_err());
}

#[test]
fn a_scope_violation_names_which_files_fell_outside() {
    let failure =
        Failure::scope_violated(vec!["crates/cli/Cargo.toml".into(), "clippy.toml".into()]);
    assert_eq!(failure.outside_scope().len(), 2);
    assert!(
        Failure::Message {
            outcome: "exit 1".into(),
        }
        .outside_scope()
        .is_empty(),
        "a failure that is not about scope names no path outside it"
    );
}
