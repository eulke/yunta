//! Scope expansion — plain violations, requests across the three modes, and human escalation to a real gate — plus re-plan.

use yunta_engine::{RunTerminal, DEFAULT_MAX_RETRIES};
use yunta_testkit::{Bench, ScriptedInteraction};

mod common;
use common::*;

#[tokio::test]
async fn writing_outside_scope_without_a_request_is_a_plain_violation_never_an_implicit_expansion()
{
    // ✓ del Plan: nada le da a un agente una vía para ampliar su propio
    // scope salvo el protocolo de request — ni un `within` que
    // técnicamente cubriría el path lo salva si nunca se escribió un
    // request. `scope_expansion: { mode: rules, within: [b.txt] }` está
    // declarado, pero el agente jamás escribe el archivo de request.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("rules", &["b.txt"], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-s", "s", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&artifacts_dir, &ledger);
    for _ in 0..=DEFAULT_MAX_RETRIES {
        fixture.push_str(
            "  - match_prompt_contains: \"task-s\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-s }\n",
        );
    }

    let (terminal, state) = bench.run(&workflow, &fixture).await;

    assert_eq!(
        state.tasks.get("task-s"),
        Some(&yunta_core::events::TaskStatus::Blocked),
        "an out-of-scope write with no request must block the task, never silently pass"
    );
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("expected the run to pause on task-s, got {other:?}"),
    }

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::ScopeExpansionRequested(p)) if p.task_id.as_str() == "task-s"
        )),
        "no scope_expansion_* event may fire when the agent never wrote a request"
    );
}

#[tokio::test]
async fn an_already_passing_proposed_criterion_is_denied_without_consulting_even_in_ask_mode() {
    // ✓ del Plan: un criterio propuesto que ya pasa se rechaza sin
    // consultar en NINGÚN modo — ni siquiera `ask`, que de otro modo
    // escalaría y pausaría el run.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt")
    );

    let request_yaml = "paths:\n  - c.txt\nreason: \"already fine, no work needed\"\nproposed_criterion:\n  cmd: \"true\"\n";
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-p\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-p }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    ));

    let (terminal, state) = bench.run(&workflow, &fixture).await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "an auto-rejected request must never pause the run, even under ask mode"
    );
    assert_eq!(
        state.tasks.get("task-p"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionDenied(p))
                if p.task_id.as_str() == "task-p" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a Denied event must be recorded");
    assert!(denied
        .denial_reason
        .as_deref()
        .unwrap_or_default()
        .contains("already passes"));

    let findings = findings_posted(&events);
    assert!(
        findings
            .iter()
            .any(|f| f.detail.contains("already fine, no work needed")),
        "the finding must carry the agent's own reason: {findings:?}"
    );
}

#[tokio::test]
async fn every_denial_becomes_a_finding_carrying_the_agent_s_reason_and_criterion() {
    // Toda denegación —acá, el default `deny` sin ningún
    // bloque `scope_expansion:` en el workflow— se convierte en un
    // finding que lleva el reason y el proposed_criterion del
    // propio agente, no una explicación inventada por el engine.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = no_scope_expansion_workflow();
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-d", "d", "a.txt", "test -f a.txt")
    );

    let request_yaml = "paths:\n  - b.txt\nreason: \"need an adjacent fix in b.txt\"\nproposed_criterion:\n  cmd: \"test -f b.txt\"\n";
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-d\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-d }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    ));

    let (terminal, state) = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("task-d"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionDenied(p))
                if p.task_id.as_str() == "task-d" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a Denied event must be recorded under the default deny mode");
    assert_eq!(
        denied.denial_reason.as_deref(),
        Some("scope_expansion mode is deny (the default)")
    );

    let findings = findings_posted(&events);
    let finding = findings
        .iter()
        .find(|f| f.detail.contains("need an adjacent fix in b.txt"))
        .expect("a finding carrying the agent's own reason must exist");
    assert_eq!(
        finding.proposed_criterion,
        Some(yunta_core::events::ProposedCriterion {
            cmd: "test -f b.txt".to_string()
        })
    );
    assert!(finding.location.contains("b.txt"));
}

#[tokio::test]
async fn a_granted_expansion_widens_what_the_final_scope_check_accepts() {
    // ✓ del Plan: "el diff final se evalúa contra scope declarado más
    // ampliaciones autorizadas" — mismo diff, mismo agente; sólo el modo
    // cambia entre las dos corridas.
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", "a.txt", "test -f a.txt")
    );
    let request_yaml = "paths:\n  - b.txt\nreason: \"small adjacent fix\"\nproposed_criterion:\n  cmd: \"test -f nonexistent-marker\"\n";
    let session = format!(
        "  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: b.txt, content: \"b\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-w }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    );

    // Granted: `rules` mode, `within` covers b.txt.
    let granted_bench = Bench::new();
    let granted_artifacts = granted_bench.run_dir().join("artifacts");
    let granted_workflow = scope_expansion_workflow("rules", &["b.txt"], None);
    let mut granted_fixture = plan_session(&granted_artifacts, &ledger);
    granted_fixture.push_str(&session);
    let (granted_terminal, granted_state) =
        granted_bench.run(&granted_workflow, &granted_fixture).await;
    assert_eq!(granted_terminal, RunTerminal::Finished);
    assert_eq!(
        granted_state.tasks.get("task-w"),
        Some(&yunta_core::events::TaskStatus::Done),
        "a granted expansion must let b.txt through the final scope check"
    );

    // Denied: same diff, `deny` mode — b.txt is never granted, so the
    // same write is now a real violation and the task never satisfies
    // its own scope check.
    let denied_bench = Bench::new();
    let denied_artifacts = denied_bench.run_dir().join("artifacts");
    let denied_workflow = scope_expansion_workflow("deny", &[], None);
    let mut denied_fixture = plan_session(&denied_artifacts, &ledger);
    for _ in 0..=DEFAULT_MAX_RETRIES {
        denied_fixture.push_str(&session);
    }
    let (_denied_terminal, denied_state) =
        denied_bench.run(&denied_workflow, &denied_fixture).await;
    assert_eq!(
        denied_state.tasks.get("task-w"),
        Some(&yunta_core::events::TaskStatus::Blocked),
        "without a grant, b.txt stays a scope violation on the same diff"
    );
}

#[tokio::test]
async fn the_request_object_is_recorded_identically_across_all_three_modes() {
    // ✓ del Plan: el request object es "idéntico en los tres modos" —
    // mismo agente, mismos paths/reason/proposed_criterion; sólo el modo
    // de la config cambia entre corridas. El evento `ScopeExpansionRequested`
    // debe grabar exactamente lo mismo en los tres casos, incluso cuando
    // el veredicto que sigue difiere.
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-g", "g", "a.txt", "test -f a.txt")
    );
    let request_yaml = "paths:\n  - c.txt\nreason: \"golden request\"\nproposed_criterion:\n  cmd: \"test -f nonexistent-marker\"\n";
    let session = format!(
        "  - match_prompt_contains: \"task-g\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: {:?}, content: {:?} }}\n    outcome: {{ type: completed, summary: did-g }}\n",
        yunta_engine::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE,
        request_yaml,
    );

    let mut requested_payloads = Vec::new();
    for mode in ["rules", "ask", "deny"] {
        let bench = Bench::new();
        let artifacts_dir = bench.run_dir().join("artifacts");
        let workflow = scope_expansion_workflow(mode, &[], None);
        let mut fixture = plan_session(&artifacts_dir, &ledger);
        fixture.push_str(&session);
        // The terminal deliberately differs by mode (rules grants, ask
        // pauses, deny blocks); this test's subject is the request event
        // recorded below, which the `unwrap_or_else` then asserts exists.
        bench.run(&workflow, &fixture).await;

        let events = bench.storage.events_for_run(&bench.run_id).unwrap();
        let requested = events
            .iter()
            .find_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::ScopeExpansionRequested(p))
                    if p.task_id.as_str() == "task-g" =>
                {
                    Some(p.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("mode `{mode}` must record a ScopeExpansionRequested event"));
        requested_payloads.push((mode, requested));
    }

    let (first_mode, first) = &requested_payloads[0];
    for (mode, payload) in &requested_payloads[1..] {
        assert_eq!(
            payload.paths, first.paths,
            "paths must be identical between `{first_mode}` and `{mode}`"
        );
        assert_eq!(payload.reason, first.reason);
        assert_eq!(payload.proposed_criterion, first.proposed_criterion);
        assert_eq!(
            payload.proposed_criterion_precheck,
            first.proposed_criterion_precheck
        );
    }
}

#[tokio::test]
async fn an_ask_mode_request_granted_by_a_human_lets_the_retry_use_the_expanded_scope() {
    // `mode: ask` with a live HumanInteraction consults instead of
    // pausing. Grant → the task returns to ready and its next attempt's
    // diff is evaluated against scope + the granted paths, which the
    // engine derives from the log's own `scope_expansion_granted.paths`.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    // Attempt 1: asks. Attempt 2 (after the human grants): same diff,
    // no new request — b.txt must now be covered by the grant on the log.
    fixture.push_str(&requesting_session("task-h"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-h }\n",
    );

    let interaction = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "grant".into(),
        by: "eulke".into(),
        free_text: None,
    });
    let (terminal, state) = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "grant must unblock the run"
    );
    assert_eq!(
        state.tasks.get("task-h"),
        Some(&yunta_core::events::TaskStatus::Done),
        "the retry's b.txt write must pass the widened scope check"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let granted = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionGranted(p))
                if p.task_id.as_str() == "task-h" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a human grant must be recorded as scope_expansion_granted");
    assert_eq!(
        granted.decided_by,
        yunta_core::events::Decider::Person { id: "eulke".into() }
    );
    assert_eq!(
        granted.paths,
        vec!["b.txt".to_string()],
        "the grant must name exactly what it authorized — self-contained audit"
    );
    // The interaction itself is on the log, same vocabulary as every
    // other gate: waiting + resolved, together.
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateWaiting(p)) if p.summary.contains("task-h")
    )));
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::GateResolved(
            yunta_core::events::GateResolvedPayload::Chosen(choice)
        )) if choice.option == "grant"
    )));
}

#[tokio::test]
async fn an_ask_mode_request_denied_by_a_human_becomes_a_finding_and_the_task_retries_in_scope() {
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-n", "n", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    // Attempt 1 asks; the human denies; attempt 2 complies with the
    // original scope (a.txt only) and succeeds.
    fixture.push_str(&requesting_session("task-n"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-n\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-n }\n",
    );

    let interaction = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "deny".into(),
        by: "eulke".into(),
        free_text: Some("out of this sprint".to_string()),
    });
    let (terminal, state) = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("task-n"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::ScopeExpansionDenied(p))
                if p.task_id.as_str() == "task-n" =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("the human denial must be recorded");
    assert_eq!(
        denied.decided_by,
        yunta_core::events::Decider::Person { id: "eulke".into() }
    );
    assert!(denied
        .denial_reason
        .as_deref()
        .unwrap_or_default()
        .contains("out of this sprint"));

    // Every denial — human ones included — becomes a finding
    // carrying the agent's own reason and proposed criterion.
    let findings = findings_posted(&events);
    let finding = findings
        .iter()
        .find(|f| f.detail.contains("adjacent fix in b.txt"))
        .expect("the denial must convert into a finding");
    assert_eq!(
        finding.proposed_criterion,
        Some(yunta_core::events::ProposedCriterion {
            cmd: "test -f nonexistent-marker".to_string()
        })
    );
}

#[tokio::test]
async fn an_ask_mode_request_with_no_surface_still_pauses_exactly_as_before() {
    // A live surface must not change the headless behavior: NoInteraction (yunta
    // test, CI) keeps degrading to a pause, with no gate recorded (an
    // unresolved question re-asks on resume, same convention as any gate).
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = scope_expansion_workflow("ask", &[], None);
    let ledger = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt")
    );
    let mut fixture = plan_session(&artifacts_dir, &ledger);
    fixture.push_str(&requesting_session("task-p"));

    let (terminal, _state) = bench.run(&workflow, &fixture).await;
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("headless ask must pause, got {other:?}"),
    }
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::GateWaiting(_))
        )),
        "an unresolved escalation must not be recorded as a published gate"
    );
}

// --- re-plan ---------------------------------------------

#[tokio::test]
async fn a_replan_preserves_an_identical_task_and_resets_one_whose_criteria_changed() {
    // ✓ del Plan (los tres): task-a se declara idéntica en ambos ledgers
    // y debe conservar `done` sin volver a correr; task-c cambia de
    // criterio (mismo id) y debe volver a `pending`; el commit de task-a
    // sigue en el worktree después del re-plan, y task-c corre sobre ese
    // mismo estado, no sobre uno revertido.
    let bench = Bench::new();
    let artifacts_dir = bench.run_dir().join("artifacts");

    let workflow = r#"
name: replan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the ledger to {{run.dir}}/artifacts/plan.yaml."
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the ledger and implement it."
    on_failure: { goto: plan, max_reroutes: 1 }
"#;

    let ledger_v1 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        // Never satisfiable by any effect a session can produce — task-c
        // exhausts its retries and blocks, which is what fails the loop
        // and triggers the reroute back to `plan`.
        task_yaml(
            "task-c",
            "Write c (bad criterion)",
            "c.txt",
            "test -f nonexistent-marker-c"
        ),
    );
    // Same id, same scope for both tasks; task-a's criterion is byte-
    // identical, task-c's is fixed to something satisfiable — the one
    // real identity change in this re-plan.
    let ledger_v2 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        task_yaml("task-c", "Write c (fixed)", "c.txt", "test -f c.txt"),
    );

    let mut fixture = format!(
        "sessions:\n  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: planned }}\n",
        artifacts_dir.display(),
        ledger_v1,
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );
    for _ in 0..=DEFAULT_MAX_RETRIES {
        fixture.push_str(
            "  - match_prompt_contains: \"task-c\"\n    outcome: { type: completed, summary: \"tried and failed\" }\n",
        );
    }
    fixture.push_str(&format!(
        "  - effects:\n      - {{ path: \"{}/plan.yaml\", content: {:?} }}\n    outcome: {{ type: completed, summary: replanned }}\n",
        artifacts_dir.display(),
        ledger_v2,
    ));
    fixture.push_str(
        "  - match_prompt_contains: \"task-c\"\n    effects:\n      - { path: c.txt, content: \"c\" }\n    outcome: { type: completed, summary: did-c }\n",
    );

    let (terminal, state) = bench.run(workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.get("task-a"),
        Some(&yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.get("task-c"),
        Some(&yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let statuses_of = |task: &str| -> Vec<yunta_core::events::TaskStatus> {
        events
            .iter()
            .filter_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::TaskStatusChanged(p))
                    if p.task_id.as_str() == task =>
                {
                    Some(p.new_status)
                }
                _ => None,
            })
            .collect()
    };

    assert_eq!(
        statuses_of("task-a"),
        vec![
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "an identical re-registration must never dispatch task-a again"
    );

    assert_eq!(
        statuses_of("task-c"),
        vec![
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Blocked,
            yunta_core::events::TaskStatus::Pending,
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "a changed criterion must reset task-c to pending and let it run again"
    );

    let registered_count = events
        .iter()
        .filter(|e| {
            matches!(e.payload(), Some(yunta_core::events::EventPayload::TaskRegistered(p)) if p.task_id.as_str() == "task-c")
        })
        .count();
    assert_eq!(
        registered_count, 2,
        "both the original and the re-planned registration must stay in the log"
    );

    let commits = commit_subjects(&bench.worktree);
    assert!(
        commits.contains(&"task task-a: Write a".to_string()),
        "task-a's committed work must survive the re-plan: {commits:?}"
    );
}
