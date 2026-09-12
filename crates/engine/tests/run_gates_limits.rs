//! Human-interaction gates, the generic internal gate, and run limits: token budget, loop-iteration cap, and inline-context threshold.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{Adapter, MockAdapter};
use yunta_core::{AdapterId, ConfigLayer, Workflow};
use yunta_engine::{
    build_manifest, create_run, execute_run, CreateRunParams, NoInteraction, NodeState, RunEnv,
    RunError, RunTerminal, DEFAULT_MAX_RETRIES,
};
use yunta_testkit::{Bench, FixedClock, ScriptedInteraction, MOCK_CONFIG};

mod common;
use common::*;

/// The option a human chose, for a resolution that is a human's choice.
fn chosen_option(resolution: &yunta_core::events::GateResolvedPayload) -> Option<&str> {
    match resolution {
        yunta_core::events::GateResolvedPayload::Chosen(choice) => Some(choice.option.as_str()),
        _ => None,
    }
}

#[tokio::test]
async fn a_gate_resolved_to_retry_reroutes_to_the_indicated_node_and_can_still_finish() {
    let bench = Bench::new();
    let interaction = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "retry".into(),
        by: "eulke".into(),
        free_text: None,
    });

    let (terminal, state) = bench
        .run_with_interaction(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
            &interaction,
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("lint"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let reroutes = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::NodeRerouted(_))
            )
        })
        .count();
    assert_eq!(
        reroutes, 2,
        "the automatic reroute plus the gate-authorized one"
    );
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateWaiting(_))
        )),
        "the escalation itself must be on the log, not just its resolution"
    );
    let resolved = events.iter().find_map(|e| match e.payload() {
        Some(yunta_core::events::EventPayload::GateResolved(p)) => Some(p),
        _ => None,
    });
    assert_eq!(resolved.and_then(chosen_option), Some("retry"));
}

#[tokio::test]
async fn a_gate_resolved_to_abort_pauses_citing_the_decision_and_free_text() {
    let bench = Bench::new();
    let interaction = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "abort".into(),
        by: "eulke".into(),
        free_text: Some("not worth chasing today".to_string()),
    });

    let (terminal, _) = bench
        .run_with_interaction(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
            &interaction,
        )
        .await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                "node `lint`'s gate was resolved to abort: not worth chasing today"
            );
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn a_gate_with_no_live_interaction_degrades_to_pausing_exactly_as_before() {
    // Regression: `NoInteraction` (what every other test in this suite
    // already uses) must reproduce the same pause behavior byte for
    // byte — a gate existing must never change what an unattended run
    // does.
    let bench = Bench::new();
    let (terminal, _) = bench
        .run(
            HOPELESS_UNTIL_RETRIED_WORKFLOW,
            HOPELESS_UNTIL_RETRIED_FIXTURE,
        )
        .await;

    match terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(
                reason,
                "node `lint` failed and its 1 re-route(s) to `fix-lint` are exhausted: exit 1: "
            );
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn an_internal_gate_approved_resolves_and_the_dag_continues() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["aprobar"]);
    let (terminal, state) = bench
        .run_with_interaction(INTERNAL_GATE_WORKFLOW, "sessions: []\n", &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    match state.nodes.get("approve") {
        Some(yunta_engine::NodeState::Finished { outcome, .. }) => {
            assert_eq!(outcome, "aprobar")
        }
        other => panic!("expected the gate finished with the chosen option, got {other:?}"),
    }
    assert_eq!(plan_run_count(&bench.worktree), 1);

    // The recorded escalation carries the declared options plus the
    // engine-appended abort, each with a tradeoff.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let waiting = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::GateWaiting(p)) => Some(p),
            _ => None,
        })
        .expect("the resolved interaction must be on the log");
    let ids: Vec<&str> = waiting.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["aprobar", "ajustar", "abort"]);
    assert!(waiting.options.iter().all(|o| !o.tradeoff.is_empty()));
    assert_eq!(waiting.summary, "Approve the plan?");
}

#[tokio::test]
async fn a_surface_answer_off_the_menu_breaks_the_run_instead_of_deciding() {
    // The engine validates what a surface returns against the menu it
    // offered: a surface that answers with an option the gate never
    // declared is a bug in the surface, reported as a broken run and
    // never recorded as the gate's outcome.
    let bench = Bench::new();
    let workflow: Workflow = serde_norway::from_str(INTERNAL_GATE_WORKFLOW).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let interaction = ScriptedInteraction::choose("whatever");
    let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();

    let error = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .expect_err("an off-menu answer is not a decision");

    let RunError::OffMenuAnswer {
        answer, offered, ..
    } = error
    else {
        panic!("expected the answer refused as off the menu, got {error:?}");
    };
    assert_eq!(answer, "whatever");
    assert_eq!(offered, "aprobar, ajustar, abort");
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateResolved(_))
        )),
        "nothing is recorded as the gate's decision"
    );
}

#[tokio::test]
async fn crash_between_gate_start_and_resolution_resumes_by_asking_again() {
    // An internal gate crashed after its `node_started` and before it
    // ever reached `gate_waiting`: the log leaves it `Running` with no
    // `external_ref`. A resume must re-ask it, not fall through every
    // scheduler section into a permanent "blocked behind unresolved
    // failures" pause that never touches the gate again.
    let bench = Bench::new();
    let workflow: Workflow = serde_norway::from_str(INTERNAL_GATE_WORKFLOW).unwrap();
    let config: ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    // Craft the crash: `plan` finished, then the gate started and the
    // engine died before recording anything else about it.
    let append = |node: &str, payload: yunta_core::events::EventPayload| {
        bench
            .storage
            .append(
                &yunta_core::events::EventDraft {
                    run_id: bench.run_id.clone(),
                    node_id: Some(node.into()),
                    payload,
                },
                &yunta_core::SystemClock,
            )
            .unwrap();
    };
    append(
        "plan",
        yunta_core::events::EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload {
            attempt: 1,
        }),
    );
    append(
        "plan",
        yunta_core::events::EventPayload::NodeFinished(yunta_core::events::NodeFinishedPayload {
            outcome: "ok".to_string(),
            tokens_used: yunta_core::events::TokenUsage::default(),
        }),
    );
    append(
        "approve",
        yunta_core::events::EventPayload::NodeStarted(yunta_core::events::NodeStartedPayload {
            attempt: 1,
        }),
    );

    let interaction = SequencedInteraction::choosing(&["aprobar"]);
    let report = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &HashMap::new(),
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();

    assert_eq!(
        report.terminal,
        RunTerminal::Finished,
        "the crashed gate is re-asked and resolved, not left stuck"
    );
    match report.state.nodes.get("approve") {
        Some(yunta_engine::NodeState::Finished { outcome, .. }) => assert_eq!(outcome, "aprobar"),
        other => panic!("expected the gate resolved after the resume, got {other:?}"),
    }
    assert!(
        matches!(
            report.state.nodes.get("ship"),
            Some(yunta_engine::NodeState::Finished { .. })
        ),
        "the node behind the gate runs once the gate resolves"
    );
}

#[tokio::test]
async fn an_internal_gate_option_mapped_in_on_reroutes_and_asks_again() {
    // Re-route semantics through a human choice: `ajustar` re-routes to
    // `plan`, plan re-runs, and the gate asks AGAIN — the second answer
    // (`aprobar`) lets the DAG continue.
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["ajustar", "aprobar"]);
    let (terminal, state) = bench
        .run_with_interaction(INTERNAL_GATE_WORKFLOW, "sessions: []\n", &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert!(matches!(
        state.nodes.get("approve"),
        Some(yunta_engine::NodeState::Finished { .. })
    ));
    assert_eq!(
        plan_run_count(&bench.worktree),
        2,
        "`ajustar` must re-run plan before the gate asks again"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(events.iter().any(|e| matches!(e.payload(), Some(yunta_core::events::EventPayload::NodeRerouted(p)) if p.to_node.as_str() == "plan" && e.node_id.as_ref().map(|n| n.as_str()) == Some("approve"))));
}

#[tokio::test]
async fn gate_reroute_records_its_origin_without_fake_counters() {
    // A gate's `on:` choice is a routing decision, not a bounded retry —
    // its `node_rerouted` says so (`origin: gate_choice`) and carries no
    // fabricated `attempt`/`max_reroutes`, unlike an `on_failure` reroute.
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["ajustar", "aprobar"]);
    let (terminal, _) = bench
        .run_with_interaction(INTERNAL_GATE_WORKFLOW, "sessions: []\n", &interaction)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let reroute = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::NodeRerouted(p))
                if e.node_id.as_ref().map(|n| n.as_str()) == Some("approve") =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("the gate's choice reroute is on the log");
    assert_eq!(
        reroute.origin,
        yunta_core::events::RerouteOrigin::GateChoice
    );
    assert_eq!(reroute.attempt, None, "a gate choice has no retry count");
    assert_eq!(reroute.max_reroutes, None, "a gate choice has no cap");
}

#[tokio::test]
async fn an_internal_gate_with_no_surface_pauses_and_a_resume_re_asks() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");
    let _ = artifacts_dir; // same Bench shape as every other e2e here
    let workflow: yunta_core::Workflow = serde_norway::from_str(INTERNAL_GATE_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_norway::from_str(MOCK_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();

    let adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::from([(
        "mock".into(),
        Arc::new(MockAdapter::from_yaml("sessions: []").unwrap()) as Arc<dyn Adapter>,
    )]);
    let first = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    match &first.terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(*reason, "gate `approve` (assignee: lead) awaits a decision")
        }
        other => panic!("headless internal gate must pause, got {other:?}"),
    }
    // Unresolved: nothing recorded (re-asks on resume, same gate convention).
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));

    let interaction = SequencedInteraction::choosing(&["aprobar"]);
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    assert_eq!(resumed.terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_run_over_its_token_budget_pauses_with_reason_budget_when_headless() {
    let bench = Bench::new();
    let (terminal, state) = bench
        .run_with_config(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => {
            assert_eq!(*reason, "budget: run spent 200 tokens with `limits.max_tokens_per_run: 100` — resume with an interactive surface to continue past the cap or abort");
        }
        other => panic!("an exhausted budget with no surface must pause, got {other:?}"),
    }
    // The corrective node never started — the cap is checked before the
    // re-route hands it work.
    assert!(matches!(
        state.nodes.get("first"),
        Some(NodeState::Failed { .. })
    ));
    assert_eq!(state.nodes.get("fix"), None);
    // Unresolved: nothing recorded (resume re-asks, same convention as
    // every other gate).
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));
}

#[tokio::test]
async fn authorizing_continue_lifts_the_cap_and_records_a_run_level_gate_pair() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["continue"]);
    let (terminal, state) = bench
        .run_full(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    // The corrective ran and control returned to `first`, which finished
    // on its retry — all of it past the cap, under the one authorization.
    for node in ["first", "fix"] {
        assert!(
            matches!(state.nodes.get(node), Some(NodeState::Finished { .. })),
            "node `{node}` should be finished, got {:?}",
            state.nodes.get(node)
        );
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let waiting = events
        .iter()
        .find(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::GateWaiting(_))
            )
        })
        .expect("the budget escalation must be recorded");
    assert_eq!(
        waiting.node_id, None,
        "the budget gate belongs to the run, not to any node"
    );
    let resolved = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::GateResolved(p)) => Some((e.node_id.clone(), p)),
            _ => None,
        })
        .expect("the authorization must be recorded");
    assert_eq!(resolved.0, None);
    assert_eq!(chosen_option(resolved.1), Some("continue"));
}

#[tokio::test]
async fn choosing_abort_on_the_budget_escalation_pauses_with_the_decision_recorded() {
    let bench = Bench::new();
    let interaction = SequencedInteraction::choosing(&["abort"]);
    let (terminal, state) = bench
        .run_full(BUDGET_WORKFLOW, BUDGET_FIXTURE, BUDGET_CONFIG, &interaction)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => assert_eq!(*reason, "budget: run spent 200 tokens with `limits.max_tokens_per_run: 100` — resume with an interactive surface to continue past the cap or abort"),
        other => panic!("abort must pause the run, got {other:?}"),
    }
    assert_eq!(state.nodes.get("fix"), None);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateResolved(p)) if chosen_option(p) == Some("abort")
        )),
        "the abort decision must be auditable in the log"
    );
}

#[tokio::test]
async fn a_run_under_its_token_budget_never_escalates() {
    let bench = Bench::new();
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  max_tokens_per_run: 1000000
"#;
    let (terminal, _) = bench
        .run_with_config(BUDGET_WORKFLOW, BUDGET_FIXTURE, config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));
}

#[tokio::test]
async fn budget_authorization_is_per_invocation_a_resume_asks_again() {
    // The first invocation pauses headless; the resume gets its own
    // "continue" — proving the ask happens per invocation and nothing in
    // the log pre-authorizes new spend.
    let bench = Bench::new();
    let workflow: yunta_core::Workflow = serde_norway::from_str(BUDGET_WORKFLOW).unwrap();
    let config: yunta_core::ConfigLayer = serde_norway::from_str(BUDGET_CONFIG).unwrap();
    let manifest = build_manifest(
        &workflow,
        &config,
        &bench.worktree,
        &bench.worktree,
        &HashMap::new(),
    )
    .unwrap();
    let run_dir = create_run(
        CreateRunParams {
            run_id: &bench.run_id,
            manifest: &manifest,
            runs_root: &bench.runs_root,
            mode: &"default".into(),
            promoted_from: None,
            artifacts: &[],
        },
        &bench.storage.async_handle(),
        &FixedClock,
    )
    .await
    .unwrap();
    let adapter = MockAdapter::from_yaml(BUDGET_FIXTURE).unwrap();
    let mut adapters: HashMap<AdapterId, Arc<dyn Adapter>> = HashMap::new();
    adapters.insert("mock".into(), Arc::new(adapter));

    let first = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &NoInteraction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    match &first.terminal {
        RunTerminal::Paused { reason } => assert_eq!(*reason, "budget: run spent 200 tokens with `limits.max_tokens_per_run: 100` — resume with an interactive surface to continue past the cap or abort"),
        other => panic!("expected the headless invocation to pause, got {other:?}"),
    }

    let interaction = SequencedInteraction::choosing(&["continue"]);
    let resumed = execute_run(RunEnv {
        run_id: &bench.run_id,
        manifest: &manifest,
        run_dir: &run_dir,
        worktree: &bench.worktree,
        adapters: &adapters,
        storage: &bench.storage.async_handle(),
        clock: std::sync::Arc::new(FixedClock),
        ids: &IDS,
        max_task_retries: DEFAULT_MAX_RETRIES,
        human_interaction: &interaction,
        forge: None,
        cancel: None,
        adapter_override: None,
        ambient: None,
    })
    .await
    .unwrap();
    assert_eq!(resumed.terminal, RunTerminal::Finished);
}

#[test]
fn session_token_budget_is_an_equal_share_bounded_by_what_remains() {
    // Fresh run, 4 non-terminal nodes: each session gets cap/4.
    assert_eq!(yunta_engine::session_token_budget(1000, 0, 4), 250);
    // Late in the run, what actually remains is the bound.
    assert_eq!(yunta_engine::session_token_budget(1000, 900, 4), 100);
    // Overspent never underflows.
    assert_eq!(yunta_engine::session_token_budget(1000, 2000, 4), 0);
    // A degenerate node count never divides by zero.
    assert_eq!(yunta_engine::session_token_budget(1000, 0, 0), 1000);
}

#[tokio::test]
async fn a_loop_over_its_iteration_cap_fails_with_the_limit_named_when_headless() {
    let bench = Bench::new();
    let fixture = loop_cap_fixture();
    let (terminal, state) = bench
        .run_with_config(LOOP_CAP_WORKFLOW, &fixture, LOOP_CAP_CONFIG)
        .await;

    match &terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("max_loop_iterations"),
            "the pause must name the limit: {reason}"
        ),
        other => panic!("an exhausted iteration cap with no surface must pause, got {other:?}"),
    }
    assert!(matches!(
        state.nodes.get("implement"),
        Some(NodeState::Failed { .. })
    ));
    // T003 never ran: iteration 3 was refused, so it stays registered
    // but untouched.
    assert_eq!(
        state.tasks.get("T003"),
        Some(&yunta_core::events::TaskStatus::Pending)
    );
}

#[tokio::test]
async fn authorizing_continue_lifts_the_iteration_cap_for_this_invocation() {
    let bench = Bench::new();
    let fixture = loop_cap_fixture();
    let interaction = SequencedInteraction::choosing(&["continue"]);
    let (terminal, state) = bench
        .run_full(LOOP_CAP_WORKFLOW, &fixture, LOOP_CAP_CONFIG, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("T003"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    // One authorization covers the whole invocation — the script had a
    // single `continue`, and iterations 3 AND 4 both ran on it.
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let resolutions = events
        .iter()
        .filter(|e| {
            matches!(
                e.payload(),
                Some(yunta_core::events::EventPayload::GateResolved(_))
            )
        })
        .count();
    assert_eq!(resolutions, 1, "asked once, not once per iteration");
}

#[tokio::test]
async fn a_ledger_within_the_default_iteration_cap_runs_unasked() {
    // No `limits:` declared — the reference default (12) covers a
    // three-task ledger with room to spare, and nothing escalates.
    let bench = Bench::new();
    let fixture = loop_cap_fixture();
    let (terminal, _) = bench.run(LOOP_CAP_WORKFLOW, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(!events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(_))
    )));
}

#[tokio::test]
async fn a_source_over_the_configured_inline_threshold_is_referenced_not_inlined() {
    let bench = Bench::new();
    std::fs::write(
        bench.worktree.join("notes.txt"),
        "MARKER-NOTES-CONTENT repeated enough to pass ten bytes",
    )
    .unwrap();
    let config = r#"
runners:
  executor:
    - { adapter: mock, model: mock-model }
limits:
  inline_context_bytes: 10
"#;
    // The only script matches the pointer wording — if the content were
    // inlined instead, no session would match and the node would fail.
    let fixture = "sessions:\n  - match_prompt_contains: \"bytes, referenced\"\n    outcome: { type: completed, summary: ok }\n";
    let (terminal, _) = bench
        .run_with_config(INLINE_CONTEXT_WORKFLOW, fixture, config)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_source_under_the_default_inline_threshold_is_inlined() {
    let bench = Bench::new();
    std::fs::write(
        bench.worktree.join("notes.txt"),
        "MARKER-NOTES-CONTENT repeated enough to pass ten bytes",
    )
    .unwrap();
    // No `limits:` — the reference default (32000 bytes) inlines it.
    let fixture = "sessions:\n  - match_prompt_contains: \"MARKER-NOTES-CONTENT\"\n    outcome: { type: completed, summary: ok }\n";
    let (terminal, _) = bench.run(INLINE_CONTEXT_WORKFLOW, fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);
}
