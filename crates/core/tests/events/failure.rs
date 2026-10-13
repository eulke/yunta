//! A node's failure as the log holds it: every variant reads back as itself,
//! and each kind of failure keeps the facts its sentence names, a command's
//! origin and output, a session's exit, and the paths a scope violation
//! refused.

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
    assert_eq!(parsed, Failure::message("exit 1: no such file"));
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
    let refused = Failure::unchanged(1, Failure::message("regression: exit 101"));
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
        .chain(loop_check_and_child_samples());
    for failure in samples {
        let json = serde_json::to_value(&failure).unwrap();
        let parsed: Failure = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(parsed, failure, "{json}");
    }
}

/// One sample of each failure that predates the typed causes.
fn plain_failure_samples() -> Vec<Failure> {
    vec![
        Failure::message("exit 1"),
        Failure::artifacts(Vec::new()),
        Failure::session_died(AdapterId::from_static("mock"), None),
        Failure::scope_violated(vec!["clippy.toml".into()]),
        Failure::scope_requested(vec![ScopeGlob::from("src/a.rs")], "needs it"),
        Failure::ScopeOwed { owed: Vec::new() },
        Failure::unset(ConfigKey::Runner),
        Failure::unchanged(1, Failure::message("regression: exit 101")),
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
    ]
}

/// One sample of a refusal and of an account's limit.
fn session_and_limit_samples() -> Vec<Failure> {
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
fn loop_check_and_child_samples() -> Vec<Failure> {
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

#[test]
fn a_scope_violation_names_which_files_fell_outside() {
    let failure =
        Failure::scope_violated(vec!["crates/cli/Cargo.toml".into(), "clippy.toml".into()]);
    assert_eq!(failure.outside_scope().len(), 2);
    assert!(
        Failure::message("exit 1").outside_scope().is_empty(),
        "a failure that is not about scope names no path outside it"
    );
}
