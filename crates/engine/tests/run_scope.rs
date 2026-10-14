//! Scope expansion — plain violations, requests across the three modes, and human escalation to a real gate — plus re-plan.

use std::path::Path;

use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction};

mod common;
use common::*;
use yunta_core::events::{
    EventPayload, GateEvent, ScopeEvent, SessionEvent, StoredEvent, TaskEvent, ToolTarget,
};
use yunta_core::{Capabilities, FenceLevel};

/// A bench whose CLI runs tools and cannot resume, and whose writes the rail
/// judges: the attempt after a grant opens fresh, on the work it left.
fn non_resuming_bench() -> Bench {
    Bench::new().with_capabilities(Capabilities {
        run_tools: true,
        fence: FenceLevel::ToolCalls,
        ..Default::default()
    })
}

/// A bench whose CLI runs tools and declares no fence, so a write outside
/// the scope lands as scripted and the final scope check is what judges it.
/// Only a CLI without a fence behaves so.
fn fenceless_bench() -> Bench {
    Bench::new().with_capabilities(Capabilities {
        run_tools: true,
        ..Default::default()
    })
}

/// Every write the log says a session was refused, by its target.
fn refused_targets(events: &[StoredEvent]) -> Vec<ToolTarget> {
    events
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Session(SessionEvent::WriteRefused(p))) => Some(p.target.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn writing_outside_scope_without_a_request_is_refused_never_an_implicit_expansion() {
    // Nothing but the request protocol lets an agent widen its own scope: a
    // `rules` loop refuses a write outside the task's scope when the agent
    // never wrote a request for it, so the write never lands.
    let bench = Bench::new();

    let workflow = scope_expansion_workflow("rules", None);
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-s", "s", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&tasks);
    fixture.push_str(
        "  - match_prompt_contains: \"task-s\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-s }\n",
    );

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-s"),
        Some(yunta_core::events::TaskStatus::Done),
        "the write outside the scope is refused, so the task closes on the work that landed"
    );
    assert!(
        !bench.worktree.join("b.txt").exists(),
        "a refused write never lands"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert_eq!(
        refused_targets(&events),
        vec![ToolTarget::of_path(Path::new("b.txt"))]
    );
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Scope(ScopeEvent::Requested(p))) if p.task_id.as_ref().is_some_and(|t| t.as_str() == "task-s")
        )),
        "no scope_expansion_* event may fire when the agent never wrote a request"
    );
}

#[tokio::test]
async fn an_already_passing_proposed_criterion_is_denied_without_consulting_even_in_ask_mode() {
    // A proposed criterion that already passes is rejected without consulting
    // anyone, in every mode, not even `ask`, which would otherwise escalate
    // and pause the run.
    let bench = Bench::new();

    let workflow = scope_expansion_workflow("ask", None);
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&tasks);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-p\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-p }}\n",
        request_step(serde_json::json!({
            "paths": ["c.txt"],
            "reason": "already fine, no work needed",
            "proposed_criterion": { "cmd": "true" },
        })),
    ));

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "an auto-rejected request must never pause the run, even under ask mode"
    );
    assert_eq!(
        state.tasks.status("task-p"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Scope(ScopeEvent::Denied(p)))
                if p.task_id.as_ref().is_some_and(|t| t.as_str() == "task-p") =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a Denied event must be recorded");
    assert_eq!(
        denied.why(),
        Some(yunta_core::expansion::Denial::PrecheckPasses)
    );

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
    // Every denial, here the default `deny` with no `scope_expansion:` block
    // in the workflow, becomes a finding that carries the agent's own reason
    // and proposed_criterion, not an explanation the engine made up.
    let bench = Bench::new();

    let workflow = no_scope_expansion_workflow();
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-d", "d", "a.txt", "test -f a.txt")
    );

    let mut fixture = plan_session(&tasks);
    fixture.push_str(&format!(
        "  - match_prompt_contains: \"task-d\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-d }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "need an adjacent fix in b.txt",
            "proposed_criterion": { "cmd": "test -f b.txt" },
        })),
    ));

    let RunReport { terminal, state } = bench.run(&workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-d"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let denied = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Scope(ScopeEvent::Denied(p)))
                if p.task_id.as_ref().is_some_and(|t| t.as_str() == "task-d") =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("a Denied event must be recorded under the default deny mode");
    assert_eq!(denied.why(), Some(yunta_core::expansion::Denial::ModeDeny));

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
    assert_eq!(
        finding.location,
        "b.txt".into(),
        "the finding locates where the agent asked to write"
    );
}

#[tokio::test]
async fn a_granted_expansion_widens_what_the_final_scope_check_accepts() {
    // The final diff is checked against the declared scope plus the
    // expansions that were granted. The same diff from the same agent runs
    // twice; only the mode changes between the two runs.
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", "a.txt", "test -f a.txt")
    );
    // The final scope check judges b.txt only if the write reaches the diff,
    // so the session writing it runs without a fence.
    let session = format!(
        "  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n      - {{ path: b.txt, content: \"b\" }}\n{}    outcome: {{ type: completed, summary: did-w }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "small adjacent fix",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        })),
    );

    // Granted: `rules` mode, a red criterion and one file in bounds.
    let granted_bench = fenceless_bench();
    let granted_workflow = scope_expansion_workflow("rules", None);
    let mut granted_fixture = plan_session(&tasks);
    granted_fixture.push_str(&session);
    let RunReport {
        terminal: granted_terminal,
        state: granted_state,
    } = granted_bench.run(&granted_workflow, &granted_fixture).await;
    assert_eq!(granted_terminal, RunTerminal::Finished);
    assert_eq!(
        granted_state.tasks.status("task-w"),
        Some(yunta_core::events::TaskStatus::Done),
        "a granted expansion must let b.txt through the final scope check"
    );

    // Denied: same diff, `deny` mode — b.txt is never granted, so the
    // same write is now a real violation and the task never satisfies
    // its own scope check.
    let denied_bench = fenceless_bench();
    let denied_workflow = scope_expansion_workflow("deny", None);
    let mut denied_fixture = plan_session(&tasks);
    denied_fixture.push_str(&session);
    let RunReport {
        state: denied_state,
        ..
    } = denied_bench.run(&denied_workflow, &denied_fixture).await;
    assert_eq!(
        denied_state.tasks.status("task-w"),
        Some(yunta_core::events::TaskStatus::Blocked),
        "without a grant, b.txt stays a scope violation on the same diff"
    );
}

#[tokio::test]
async fn the_request_object_is_recorded_identically_across_all_three_modes() {
    // The request object is identical across the three modes: the same agent
    // asks with the same paths, reason and proposed_criterion, and only the
    // mode in the config changes between runs. The `ScopeExpansionRequested`
    // event must record exactly the same content in all three cases, even
    // when the verdict that follows differs. The one part that differs is the
    // pre-check: a request the project denies in its call is refused before its
    // criterion runs, so the deny mode records none.
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-g", "g", "a.txt", "test -f a.txt")
    );
    let session = format!(
        "  - match_prompt_contains: \"task-g\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: did-g }}\n",
        request_step(serde_json::json!({
            "paths": ["c.txt"],
            "reason": "golden request",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        })),
    );

    let mut requested_payloads = Vec::new();
    for mode in ["rules", "ask", "deny"] {
        let bench = Bench::new();
        let workflow = scope_expansion_workflow(mode, None);
        let mut fixture = plan_session(&tasks);
        fixture.push_str(&session);
        // The terminal deliberately differs by mode (rules grants, ask
        // pauses, deny blocks); this test's subject is the request event
        // recorded below, which the `unwrap_or_else` then asserts exists.
        bench.run(&workflow, &fixture).await;

        let events = bench.storage.events_for_run(&bench.run_id).unwrap();
        let requested = events
            .iter()
            .find_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::Scope(ScopeEvent::Requested(p)))
                    if p.task_id.as_ref().is_some_and(|t| t.as_str() == "task-g") =>
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
        match *mode {
            "deny" => assert_eq!(
                payload.proposed_criterion_precheck, None,
                "a request denied in its call runs no criterion"
            ),
            _ => assert_eq!(
                payload.proposed_criterion_precheck,
                first.proposed_criterion_precheck
            ),
        }
    }
}

/// How many sessions the `implement` loop opened.
fn implement_sessions(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|e| {
            e.node_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "implement")
                && matches!(
                    e.payload(),
                    Some(yunta_core::events::EventPayload::Session(
                        yunta_core::events::SessionEvent::Opened(_)
                    ))
                )
        })
        .count()
}

/// A grant the engine makes itself changes what the next session may
/// write — under a fence, the session that asked could not write it — so
/// one more attempt follows on its own. It is the only one that does: the
/// attempt after it, like any red attempt, blocks the task.
#[tokio::test]
async fn an_engine_granted_expansion_is_followed_by_the_one_attempt_it_makes_different() {
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-w", "w", "a.txt", "test -f a.txt && test -f b.txt")
    );
    // The attempt that asks writes what its scope allows and the request;
    // the write to b.txt is what a fence would have refused.
    let asks = format!(
        "  - match_prompt_contains: \"task-w\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: asked }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "the criterion needs it",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        })),
    );
    let writes = "  - match_prompt_contains: \"task-w\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: wrote }\n";
    let bench = Bench::new();
    let mut fixture = plan_session(&tasks);
    fixture.push_str(&asks);
    fixture.push_str(writes);
    let RunReport { terminal, state } = bench
        .run(&scope_expansion_workflow("rules", None), &fixture)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-w"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        implement_sessions(&bench),
        2,
        "the grant's attempt, and no other"
    );

    // Denied, nothing changed for a next session: the task blocks after
    // the one attempt.
    let bench = Bench::new();
    let mut fixture = plan_session(&tasks);
    fixture.push_str(&asks);
    fixture.push_str(writes);
    let RunReport { state, .. } = bench
        .run(&scope_expansion_workflow("deny", None), &fixture)
        .await;
    assert_eq!(
        state.tasks.status("task-w"),
        Some(yunta_core::events::TaskStatus::Blocked)
    );
    assert_eq!(implement_sessions(&bench), 1);
}

#[tokio::test]
async fn an_ask_mode_request_granted_by_a_human_lets_the_retry_use_the_expanded_scope() {
    // `mode: ask` with a live HumanInteraction consults instead of
    // pausing. Grant → the task returns to ready and its next attempt's
    // diff is evaluated against scope + the granted paths, which the
    // engine derives from the log's own `scope_expansion_granted.paths`.
    let bench = Bench::new();

    let workflow = scope_expansion_workflow("ask", None);
    // The criterion needs b.txt, so the first attempt ends unclosed with the
    // request pending: the fresh session it opens writes the same, and the
    // request is owed a person's decision.
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.txt", "test -f a.txt && test -f b.txt")
    );
    let mut fixture = plan_session(&tasks);
    // Attempt 1: asks. Fresh session: asks again, writes the same. After the
    // human grants: the same diff, no new request — b.txt must now be covered
    // by the grant on the log.
    fixture.push_str(&requesting_session("task-h"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-h }\n",
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: did-h }\n",
    );

    let interaction = ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "grant".into(),
        by: "eulke".into(),
        free_text: None,
    });
    let RunReport { terminal, state } = bench
        .run_with_interaction(&workflow, &fixture, &interaction)
        .await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "grant must unblock the run"
    );
    assert_eq!(
        state.tasks.status("task-h"),
        Some(yunta_core::events::TaskStatus::Done),
        "the retry's b.txt write must pass the widened scope check"
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let granted = events
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Scope(ScopeEvent::Granted(p)))
                if p.task_id.as_ref().is_some_and(|t| t.as_str() == "task-h") =>
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
        vec![yunta_core::ScopeGlob::from("b.txt")],
        "the grant must name exactly what it authorized — self-contained audit"
    );
    // The interaction itself is on the log, same vocabulary as every
    // other gate: waiting + resolved, together.
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(p))) if matches!(p.summary(), yunta_core::events::Claim::Written(text) if text.contains("task-h"))
    )));
    assert!(events.iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Gates(GateEvent::Resolved(
            yunta_core::events::GateResolvedPayload::Chosen(choice)
        ))) if choice.option == "grant"
    )));
}

#[tokio::test]
async fn an_ask_mode_request_with_no_surface_still_pauses_exactly_as_before() {
    // A live surface must not change the headless behavior: NoInteraction (yunta
    // test, CI) keeps degrading to a pause, with no gate recorded (an
    // unresolved question re-asks on resume, same convention as any gate).
    let bench = Bench::new();

    let workflow = scope_expansion_workflow("ask", None);
    // The criterion needs b.txt, which the request asks for, so the attempt
    // ends unclosed with the request pending: the fresh session it opens
    // writes nothing, and the request is owed a person.
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-p", "p", "a.txt", "test -f a.txt && test -f b.txt")
    );
    let mut fixture = plan_session(&tasks);
    fixture.push_str(&requesting_session("task-p"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-p\"\n    outcome: { type: completed, summary: retried }\n",
    );

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(&workflow, &fixture).await;
    match terminal {
        RunTerminal::Paused { .. } => {}
        other => panic!("headless ask must pause, got {other:?}"),
    }
    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e.payload(),
            Some(yunta_core::events::EventPayload::Gates(GateEvent::Waiting(
                _
            )))
        )),
        "an unresolved escalation must not be recorded as a published gate"
    );
}

// --- re-plan ---------------------------------------------

#[tokio::test]
async fn a_replan_preserves_an_identical_task_and_resets_one_whose_criteria_changed() {
    // task-a is declared identically in both tasks documents, so it keeps
    // `done` without running again. task-c changes its criterion under the
    // same id, so it goes back to `pending`. task-a's commit stays in the
    // worktree after the re-plan, and task-c runs on that same state, not on
    // a reverted one.
    let bench = Bench::new();

    let workflow = r#"
name: replan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
    on_failure: { goto: plan, max_reroutes: 1 }
"#;

    let tasks_v1 = format!(
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
    let tasks_v2 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        task_yaml("task-c", "Write c (fixed)", "c.txt", "test -f c.txt"),
    );

    let mut fixture = plan_session(&tasks_v1);
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-c\"\n    outcome: { type: completed, summary: \"tried and failed\" }\n",
    );
    fixture.push_str(&tasks_session(&tasks_v2, "replanned"));
    fixture.push_str(
        "  - match_prompt_contains: \"task-c\"\n    effects:\n      - { path: c.txt, content: \"c\" }\n    outcome: { type: completed, summary: did-c }\n",
    );

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-a"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.status("task-c"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let statuses_of = |task: &str| -> Vec<yunta_core::events::TaskStatus> {
        events
            .iter()
            .filter_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(p)))
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
            matches!(e.payload(), Some(yunta_core::events::EventPayload::Tasks(TaskEvent::Registered(p))) if p.task_id.as_str() == "task-c")
        })
        .count();
    assert_eq!(
        registered_count, 2,
        "both the original and the re-planned registration must stay in the log"
    );

    let commits = bench.commit_subjects();
    assert!(
        commits.contains(&"task task-a: Write a".to_string()),
        "task-a's committed work must survive the re-plan: {commits:?}"
    );
}

#[tokio::test]
async fn a_successor_s_replan_keeps_done_what_it_kept_and_resets_what_it_recut() {
    // A run born holding what another run finished, whose own planner
    // then hands over a document cutting one of those tasks
    // differently: the identical one stays done without a session, the
    // re-cut one starts over.
    let tasks_v1 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        task_yaml("task-c", "Write c", "c.txt", "test -f nonexistent-marker-c"),
    );
    let document: yunta_core::TasksFile =
        yunta_core::shape::read(tasks_v1.as_bytes(), "tasks").unwrap();
    let bytes = yunta_core::shape::render(&document).unwrap().into_bytes();

    let source = yunta_core::RunId::from("run-source");
    let bench = Bench::new().born_holding(vec![yunta_engine::BirthArtifact {
        artifact: yunta_core::events::ArtifactId::Interpreted {
            kind: yunta_core::ArtifactKind::Tasks,
        },
        origin: yunta_engine::BirthOrigin::Inherited {
            run: source.clone(),
            producer: None,
        },
        bytes,
    }]);
    // The source run's own log: both tasks registered, both done at a
    // commit this bench's tree already carries — the successor works in
    // the tree the predecessor left, so its `done` answers here.
    let landed: yunta_core::CommitSha =
        yunta_testkit::git_output(&bench.worktree, &["rev-parse", "HEAD"])
            .parse()
            .unwrap();
    let planted = yunta_testkit::SourceLog::open(
        &bench.storage,
        &source,
        std::sync::Arc::new(yunta_testkit_core::FixedClock),
    );
    for task in &document.tasks {
        planted.task(task, yunta_core::events::TaskStatus::Done, Some(&landed));
    }

    let workflow = r#"
name: replan-successor
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;
    // `task-a` comes back byte-identical; `task-c` keeps its id and
    // scope but gets a criterion it can actually satisfy.
    let tasks_v2 = format!(
        "tasks:\n{}{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt"),
        task_yaml("task-c", "Write c (fixed)", "c.txt", "test -f c.txt"),
    );
    let mut fixture = plan_session(&tasks_v2);
    // A session for `task-a` the run must never need: it stays
    // unconsumed exactly because the task crossed over done.
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );
    fixture.push_str(
        "  - match_prompt_contains: \"task-c\"\n    effects:\n      - { path: c.txt, content: \"c\" }\n    outcome: { type: completed, summary: did-c }\n",
    );

    let RunReport { terminal, state } = bench.run(workflow, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-a"),
        Some(yunta_core::events::TaskStatus::Done)
    );
    assert_eq!(
        state.tasks.status("task-c"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let statuses_of = |task: &str| -> Vec<yunta_core::events::TaskStatus> {
        events
            .iter()
            .filter_map(|e| match e.payload() {
                Some(yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(p)))
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
        vec![yunta_core::events::TaskStatus::Done],
        "a task the source finished and the re-plan cut identically is never dispatched here"
    );
    assert_eq!(
        statuses_of("task-c"),
        vec![
            yunta_core::events::TaskStatus::Done,
            yunta_core::events::TaskStatus::Pending,
            yunta_core::events::TaskStatus::Running,
            yunta_core::events::TaskStatus::Done,
        ],
        "a re-cut task loses the done it crossed with and runs again"
    );
    assert!(
        !bench
            .commit_subjects()
            .contains(&"task task-a: Write a".to_string()),
        "no session ever ran for task-a here"
    );
}

#[tokio::test]
async fn an_integrated_task_records_the_commit_its_work_landed_at() {
    let bench = Bench::new();
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-a", "Write a", "a.txt", "test -f a.txt")
    );
    let workflow = r#"
name: one-task
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Read your task from the tasks document and implement it."
"#;
    let mut fixture = plan_session(&tasks);
    fixture.push_str(
        "  - match_prompt_contains: \"task-a\"\n    effects:\n      - { path: a.txt, content: \"a\" }\n    outcome: { type: completed, summary: did-a }\n",
    );

    let RunReport {
        terminal,
        state: _state,
    } = bench.run(workflow, &fixture).await;
    assert_eq!(terminal, RunTerminal::Finished);

    let events = bench.storage.events_for_run(&bench.run_id).unwrap();
    let placed: Vec<Option<yunta_core::CommitSha>> = events
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Tasks(TaskEvent::StatusChanged(p)))
                if p.new_status == yunta_core::events::TaskStatus::Done =>
            {
                Some(p.commit.clone())
            }
            _ => None,
        })
        .collect();
    let head: yunta_core::CommitSha =
        yunta_testkit::git_output(&bench.worktree, &["rev-parse", "HEAD"])
            .parse()
            .unwrap();
    assert_eq!(
        placed,
        vec![Some(head)],
        "the done names the commit the run's tree carried after integrating the task"
    );
}

// --- which tree an audit starts from ----------------------------------------

/// What a node wrote is what its own starting tree says it wrote. A
/// predecessor that left uncommitted work is part of the state this node
/// starts from, not something it answers for.
#[tokio::test]
async fn a_node_is_not_blamed_for_what_a_predecessor_left_behind() {
    let bench = Bench::new();
    let workflow = r#"
name: predecessor
nodes:
  - id: first
    kind: bash
    run: "echo one > loose.txt"
  - id: second
    kind: bash
    depends_on: [first]
    scope: ["bar/**"]
    run: "mkdir -p bar && echo two > bar/x.txt"
"#;

    let RunReport { terminal, state } = bench
        .run(workflow, "dialect: claude-code\nsessions: []")
        .await;

    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "`second` wrote only inside its scope: {terminal:?}"
    );
    assert!(
        matches!(
            state.nodes.state("second"),
            Some(yunta_engine::NodeState::Finished { .. })
        ),
        "got {:?}",
        state.nodes.state("second")
    );
}

/// The reverse holds too, and it is what a starting point taken from a list
/// of paths, rather than from the tree, would let through: a file the
/// predecessor left dirty that this node *also* touches still belongs to
/// this node.
#[tokio::test]
async fn a_node_that_touches_what_a_predecessor_left_is_still_judged_for_it() {
    let bench = Bench::new();
    let workflow = r#"
name: predecessor-touched
nodes:
  - id: first
    kind: bash
    run: "echo one > loose.txt"
  - id: second
    kind: bash
    depends_on: [first]
    scope: ["bar/**"]
    run: "mkdir -p bar && echo two > bar/x.txt && echo mine > loose.txt"
"#;

    let RunReport { terminal, state } = bench
        .run(workflow, "dialect: claude-code\nsessions: []")
        .await;

    assert!(
        !matches!(terminal, RunTerminal::Finished),
        "writing outside scope keeps failing even when the file was already dirty: {terminal:?}"
    );
    assert!(
        matches!(
            state.nodes.state("second"),
            Some(yunta_engine::NodeState::Failed { .. })
        ),
        "got {:?}",
        state.nodes.state("second")
    );
}

/// The starting point is a fact of the log, not memory of one invocation: an
/// attempt that restarts is judged from where the first attempt began,
/// because the partial work left by earlier attempts is its own.
#[tokio::test]
async fn an_attempt_that_restarts_is_judged_from_where_its_first_attempt_began() {
    let bench = Bench::new();
    let workflow = r#"
name: restarted
nodes:
  - id: flaky
    kind: bash
    scope: ["ok/**"]
    run: "mkdir -p ok && echo a > ok/a.txt && echo stray > stray.txt"
"#;

    let RunReport { terminal, state } = bench
        .run(workflow, "dialect: claude-code\nsessions: []")
        .await;

    // The write outside scope is its own and fails, naming `stray.txt`, not
    // the whole tree of the run.
    assert!(!matches!(terminal, RunTerminal::Finished), "{terminal:?}");
    let scoped = bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Node(
                yunta_core::events::NodeEvent::ScopeChecked(p),
            )) => Some(p.clone()),
            _ => None,
        })
        .expect("the node was audited");
    assert_eq!(
        scoped.violations,
        vec![std::path::PathBuf::from("stray.txt")],
        "diff: {:?}",
        scoped.diff
    );
    assert!(matches!(
        state.nodes.state("flaky"),
        Some(yunta_engine::NodeState::Failed { .. })
    ));
}

/// The starting tree is recorded in the log, where a replay finds it.
#[tokio::test]
async fn a_start_records_the_tree_its_attempt_began_from() {
    let bench = Bench::new();
    let workflow = "name: recorded\nnodes:\n  - id: work\n    kind: bash\n    run: \"true\"\n";
    bench
        .run(workflow, "dialect: claude-code\nsessions: []")
        .await;

    let started = bench
        .events()
        .iter()
        .find_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Node(
                yunta_core::events::NodeEvent::Started(p),
            )) => Some(p.clone()),
            _ => None,
        })
        .expect("the node started");
    assert!(
        started.from_tree.is_some(),
        "a start names the tree it starts from: {started:?}"
    );
}

/// A task whose session asks for `b.txt` and, as a fence would have it,
/// writes only `a.txt`; its criterion needs both. After a person grants,
/// `answered` scripts what the session does next.
fn asks_then(answered: &str) -> (String, String) {
    let tasks = format!(
        "tasks:\n{}",
        task_yaml("task-h", "h", "a.txt", "test -f a.txt && test -f b.txt")
    );
    let asks = format!(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - {{ path: a.txt, content: \"a\" }}\n{}    outcome: {{ type: completed, summary: asked }}\n",
        request_step(serde_json::json!({
            "paths": ["b.txt"],
            "reason": "the criterion needs b.txt",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        })),
    );
    // The fresh session the pending request opens writes nothing, so the
    // request stays owed a person's decision.
    let fresh =
        "  - match_prompt_contains: \"task-h\"\n    outcome: { type: completed, summary: fresh }\n";
    let fixture = plan_session(&tasks) + &asks + fresh + answered;
    (scope_expansion_workflow("ask", None), fixture)
}

fn grants() -> ScriptedInteraction {
    ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "grant".into(),
        by: "eulke".into(),
        free_text: None,
    })
}

/// The answer is the one thing that changed for the session the cycle
/// ended in: it is picked back up in the checkout it saw, holding its work,
/// told what was granted and where to read its scope — not started over.
#[tokio::test]
async fn a_granted_request_resumes_the_session_the_cycle_ended_in() {
    let (workflow, fixture) = asks_then(
        "  - match_prompt_contains: \"was granted: b.txt\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: wrote-b }\n",
    );
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(&workflow, &fixture, &grants())
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(
        state.tasks.status("task-h"),
        Some(yunta_core::events::TaskStatus::Done)
    );

    // The request is logged once though three sessions read it: the fresh
    // session after the one that asked carries the same request, no grant.
    let requested = bench
        .events()
        .iter()
        .filter(|e| e.payload().map(|p| p.kind_name()) == Some("scope_expansion_requested"))
        .count();
    assert_eq!(requested, 1, "the request is logged once, not per session");
    // The answer picks back up the session the cycle ended in, in the
    // checkout all three sessions share.
    let opened = task_sessions(&bench, "task-h");
    assert_eq!(opened.len(), 3, "{opened:?}");
    assert_eq!(opened[1].continues, None);
    assert_eq!(opened[2].continues.as_ref(), Some(&opened[1].session_id));
    assert_eq!(
        bench.mock().resumes_seen(),
        vec![opened[1].session_id.clone()]
    );
    let requests = bench.mock().requests_seen();
    let in_units: Vec<_> = requests
        .iter()
        .filter(|request| request.cwd.to_string_lossy().contains("/pool/"))
        .map(|request| request.cwd.clone())
        .collect();
    assert_eq!(in_units.len(), 3, "{in_units:?}");
    assert!(
        in_units.iter().all(|cwd| *cwd == in_units[0]),
        "the session picks back up in the checkout it saw"
    );
    let resumed = requests.last().unwrap();
    assert!(resumed.prompt.contains("yunta_task"), "{}", resumed.prompt);
}

/// An adapter that cannot resume still keeps the work: a fresh session
/// opens in the same checkout on the brief, and the log says the
/// conversation was not continued.
#[tokio::test]
async fn an_adapter_that_cannot_resume_opens_fresh_on_the_same_work() {
    let (workflow, fixture) = asks_then(
        "  - match_prompt_contains: \"task-h\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: wrote-b }\n",
    );
    let bench = non_resuming_bench();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&workflow, &fixture, &grants())
        .await;
    assert_eq!(
        terminal,
        RunTerminal::Finished,
        "a.txt from the first session is still there"
    );

    // The asking session, then the fresh one the cycle opens, then the
    // fresh one the answer reopens: an adapter that cannot resume never
    // continues any of them.
    let opened = task_sessions(&bench, "task-h");
    assert_eq!(opened.len(), 3, "{opened:?}");
    assert_eq!(opened[2].continues, None);
    assert!(bench.mock().resumes_seen().is_empty());
    assert!(bench.events().iter().any(|e| matches!(
        e.payload(),
        Some(yunta_core::events::EventPayload::Session(
            yunta_core::events::SessionEvent::CapabilityDegraded(_)
        ))
    )));
}

/// The sessions `task` opened, in order.
fn task_sessions(bench: &Bench, task: &str) -> Vec<yunta_core::events::AgentSessionOpenedPayload> {
    bench
        .events()
        .iter()
        .filter_map(|e| match e.payload() {
            Some(yunta_core::events::EventPayload::Session(
                yunta_core::events::SessionEvent::Opened(p),
            )) if p.task_id.as_ref().is_some_and(|id| id.as_str() == task) => Some(p.clone()),
            _ => None,
        })
        .collect()
}

/// An engine grant changes what the next attempt may write, and nothing
/// else: that attempt resumes the session that asked, in the same unit.
#[tokio::test]
async fn the_attempt_after_an_engine_grant_resumes_the_session_that_asked() {
    let (_, fixture) = asks_then(
        "  - match_prompt_contains: \"was granted: b.txt\"\n    effects:\n      - { path: b.txt, content: \"b\" }\n    outcome: { type: completed, summary: wrote-b }\n",
    );
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run(&scope_expansion_workflow("rules", None), &fixture)
        .await;
    assert_eq!(terminal, RunTerminal::Finished);
    let opened = task_sessions(&bench, "task-h");
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert_eq!(opened[1].continues.as_ref(), Some(&opened[0].session_id));
    assert_eq!(
        bench.mock().resumes_seen(),
        vec![opened[0].session_id.clone()]
    );
}
