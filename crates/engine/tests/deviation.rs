//! A task session that cannot build what the plan declares says so, and
//! its task does not close on it until a person answers. A criterion that
//! no work on the plan can pass is declared unpassable instead, and the
//! planner is routed to rewrite the plan.

mod common;

use common::*;
use yunta_core::events::{
    DepartsFrom, EventPayload, Failure, GateEvent, PauseReason, SessionEvent, TaskCause, TaskEvent,
    TaskStatus, TaskStop,
};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction, MOCK_CONFIG};

/// A plan whose one task owns the `Greeting` shape and must write
/// `greeting.txt`.
const PLAN: &str = "\
shapes:
  - { name: Greeting, owner: greet, file: greeting.txt, code: \"Hello, and good night\" }
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
";

/// [`PLAN`] as a person reviewing it reads it: saying what it changes,
/// how, and what proves it.
const REVIEWED_PLAN: &str = "\
summary: \"Greet\"
description: \"Writes the greeting.\"
shapes:
  - { name: Greeting, owner: greet, file: greeting.txt, code: \"Hello, and good night\" }
tasks:
  - id: greet
    title: \"Write the greeting\"
    description: \"Writes greeting.txt.\"
    scope: [\"greeting.txt\"]
    changes: [{ at: greeting.txt, what: \"the greeting\", code: \"hello\" }]
    outcome: \"greeting.txt says good night\"
    criteria:
      - { cmd: \"test -f greeting.txt\", proves: \"the greeting exists\" }
";

/// The task session: writes the file — the criterion passes — and
/// declares that what it wrote departs from the plan's `Greeting`.
fn departs(from: &str) -> String {
    format!(
        "  - match_prompt_contains: \"`greet`\"
    effects:
      - {{ path: greeting.txt, content: \"Hello\" }}
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
        arguments:
          kind: departure
          from: {from}
          planned: \"Hello, and good night\"
          instead: \"Hello\"
          why: \"the second half needs the clock, which this task may not read\"
    outcome: {{ type: completed, summary: wrote-hello }}
"
    )
}

fn fixture(then: &str) -> String {
    fixture_of(PLAN, then)
}

fn fixture_of(plan: &str, then: &str) -> String {
    plan_session(plan) + &departs("{ shape: Greeting }") + then
}

const WORKFLOW: &str = r#"
name: departs
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
"#;

fn answering(option: &str, said: Option<&str>) -> ScriptedInteraction {
    ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: option.into(),
        by: "lead".into(),
        free_text: said.map(str::to_string),
    })
}

/// How many sessions opened for the task.
fn task_sessions(bench: &Bench) -> usize {
    bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(p)))
                    if p.task_id.as_ref().is_some_and(|task| task.as_str() == "greet")
            )
        })
        .count()
}

/// Whether `stopped` holds `greet` stopped on a departure a person still owes.
fn owes_departure(stopped: &[TaskStop]) -> bool {
    stopped.iter().any(|stop| {
        stop.task_id.as_str() == "greet" && matches!(stop.cause, TaskCause::DeviationOwed { .. })
    })
}

#[tokio::test]
async fn a_departure_accepted_closes_the_task_on_its_work_without_another_session() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(WORKFLOW, &fixture(""), &answering("accept", None))
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert_eq!(task_sessions(&bench), 1);
    let events = bench.events();
    let declared = events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Tasks(TaskEvent::DeviationDeclared(p))) => Some(p.clone()),
            _ => None,
        })
        .expect("the departure is on the log, in the session's words");
    assert_eq!(declared.from, DepartsFrom::Shape("Greeting".to_string()));
    assert_eq!(
        declared.why,
        "the second half needs the clock, which this task may not read"
    );
    let asked: Vec<String> = events
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(p))) => Some(
                p.options()
                    .iter()
                    .map(|option| option.id.to_string())
                    .collect(),
            ),
            _ => None,
        })
        .expect("a person was asked");
    assert_eq!(asked, ["accept", "send-back", "abort"]);
}

#[tokio::test]
async fn a_departure_sent_back_resumes_the_session_with_what_the_person_said() {
    let then = "  - match_prompt_contains: \"sent back the departure\"
    effects:
      - { path: greeting.txt, content: \"Hello, and good night\" }
    outcome: { type: completed, summary: wrote-both }
";
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(
            WORKFLOW,
            &fixture(then),
            &answering("send-back", Some("say good night without the clock")),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert_eq!(task_sessions(&bench), 2);
    assert_eq!(bench.mock().resumes_seen().len(), 1, "the same session");
    let resumed = bench.mock().requests_seen().last().cloned().unwrap();
    assert!(
        resumed.prompt.contains("say good night without the clock"),
        "{}",
        resumed.prompt
    );
}

#[tokio::test]
async fn a_departure_no_one_answers_parks_the_run_owing_it() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench.run(WORKFLOW, &fixture("")).await;

    let RunTerminal::Paused {
        reason:
            PauseReason::NodeFailed {
                failure: Failure::TasksStopped { stopped },
                ..
            },
    } = terminal
    else {
        panic!("expected the run to pause owing the departure, got {terminal:?}");
    };
    assert!(owes_departure(&stopped), "{stopped:?}");
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Blocked));
}

#[tokio::test]
async fn an_abort_on_a_departure_pauses_without_a_missing_answer_and_is_asked_again() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(WORKFLOW, &fixture(""), &answering("abort", None))
        .await;
    let RunTerminal::Paused { reason } = terminal else {
        panic!("expected a pause, got {terminal:?}");
    };
    assert!(
        matches!(reason, PauseReason::GateAborted { .. }),
        "{reason:?}"
    );
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Blocked));
    let RunReport { terminal, .. } = bench.wake_answering(&answering("accept", None)).await;
    assert_eq!(terminal, RunTerminal::Finished);
}

#[tokio::test]
async fn a_departure_from_what_the_plan_does_not_hold_is_refused_and_not_recorded() {
    let bench = Bench::new();
    let fixture = plan_session(PLAN)
        + &departs("{ shape: Farewell }").replace(
            "        tool: yunta_declare_deviation\n",
            "        tool: yunta_declare_deviation\n        expect: refused\n",
        );
    let RunReport { terminal, state } = bench.run(WORKFLOW, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Tasks(TaskEvent::DeviationDeclared(_)))
        )),
        "nothing the plan does not hold is recorded as a departure from it"
    );
}

#[tokio::test]
async fn a_node_reading_the_departures_sees_each_one_and_its_answer_and_nothing_else() {
    let workflow = format!(
        "{WORKFLOW}  - id: conform
    kind: prompt
    runner: planner
    depends_on: [implement]
    prompt: \"Hold the work to the plan.\"
    context:
      - run-events: {{ filter: deviations }}
"
    );
    let then = "  - match_prompt_contains: \"Hold the work to the plan.\"
    outcome: { type: completed, summary: held }
";
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&workflow, &fixture(then), &answering("accept", None))
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let read = bench.mock().requests_seen().last().cloned().unwrap().prompt;
    assert!(read.contains("\"deviation_declared\""), "{read}");
    assert!(read.contains("\"deviation_resolved\""), "{read}");
    assert!(
        read.contains("the second half needs the clock, which this task may not read"),
        "{read}"
    );
    assert!(!read.contains("\"task_status_changed\""), "{read}");
}

/// A person who grants whatever scope a task asks for, and is away for
/// anything else.
struct GrantsScopeOnly;

#[async_trait::async_trait]
impl yunta_engine::HumanInteraction for GrantsScopeOnly {
    async fn resolve(
        &self,
        escalation: &yunta_core::events::GateWaitingPayload,
    ) -> Option<yunta_core::events::HumanChoice> {
        escalation
            .options()
            .iter()
            .any(|option| option.id.as_str() == "grant")
            .then(|| yunta_core::events::HumanChoice {
                option: "grant".into(),
                by: "lead".into(),
                free_text: None,
            })
    }
}

#[tokio::test]
async fn a_departure_declared_beside_a_scope_request_is_still_owed_once_the_scope_is_granted() {
    let workflow = WORKFLOW.replace(
        "    until: all_tasks_complete\n",
        "    until: all_tasks_complete\n    scope_expansion: { mode: ask }\n",
    );
    // The session writes extra.txt, which the rail refuses, and then asks for it.
    let asks = format!(
        "      - type: run_tool
        tool: yunta_request_scope_expansion
        arguments: {}
",
        serde_json::json!({
            "paths": ["extra.txt"],
            "reason": "the greeting needs a second file",
            "proposed_criterion": { "cmd": "test -f nonexistent-marker" },
        }),
    );
    let fixture = plan_session(PLAN)
        + &departs("{ shape: Greeting }")
            .replace(
                "      - { path: greeting.txt, content: \"Hello\" }\n",
                "      - { path: greeting.txt, content: \"Hello\" }\n      - { path: extra.txt, content: \"x\" }\n",
            )
            .replace(
                "    outcome: { type: completed, summary: wrote-hello }\n",
                &format!("{asks}    outcome: {{ type: completed, summary: wrote-hello }}\n"),
            )
        + "  - match_prompt_contains: \"`greet`\"
    effects:
      - { path: greeting.txt, content: \"Hello\" }
      - { path: extra.txt, content: \"x\" }
    outcome: { type: completed, summary: wrote-hello-again }
";
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(&workflow, &fixture, &GrantsScopeOnly)
        .await;

    let RunTerminal::Paused {
        reason:
            PauseReason::NodeFailed {
                failure: Failure::TasksStopped { stopped },
                ..
            },
    } = terminal
    else {
        panic!("expected the run to pause owing the departure, got {terminal:?}");
    };
    assert!(owes_departure(&stopped), "{stopped:?}");
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Blocked));
}

#[tokio::test]
async fn a_plan_shown_after_its_work_carries_the_departures_a_person_accepted() {
    let workflow = format!(
        "{WORKFLOW}  - id: ship
    kind: gate
    depends_on: [implement]
    assignee: lead
    message: \"Ship it?\"
    shows: [{{ node: plan, kind: tasks }}]
"
    );
    let interaction = SequencedInteraction::answering(vec![
        ("accept", Some("the clock can wait")),
        ("approve", None),
    ]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(&workflow, &fixture_of(REVIEWED_PLAN, ""), &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let shown = interaction.shown();
    let at_ship = shown.last().expect("the gate was asked");
    let yunta_engine::ShownContent::Tasks(review) = &at_ship[0].content else {
        panic!("the plan is shown as its tasks: {at_ship:?}");
    };
    let [departure] = review.departed.as_slice() else {
        panic!("the one departure accepted: {:?}", review.departed);
    };
    assert_eq!(departure.declared.task_id.as_str(), "greet");
    assert_eq!(
        departure.declared.from,
        DepartsFrom::Shape("Greeting".to_string())
    );
    assert_eq!(departure.declared.instead, "Hello");
    assert_eq!(departure.said.as_deref(), Some("the clock can wait"));
}

#[tokio::test]
async fn a_criterion_that_passes_or_belongs_to_the_suite_is_never_declared_unpassable() {
    let config = format!("{MOCK_CONFIG}baseline:\n  suite: \"true\"\n");
    let plan = "\
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
      - cmd: \"true\"
        type: guard
";
    let fixture = plan_session(plan)
        + "  - match_prompt_contains: \"`greet`\"
    effects:
      - { path: greeting.txt, content: \"Hello\" }
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
        expect: refused
        arguments: { kind: unpassable, criterion: \"test -f greeting.txt\", why: \"it passes\" }
      - type: run_tool
        tool: yunta_declare_deviation
        expect: refused
        arguments: { kind: unpassable, criterion: \"true\", why: \"the suite holds it\" }
    outcome: { type: completed, summary: declared }
";
    let bench = Bench::new();
    let RunReport { terminal, state } = bench.run_with_config(WORKFLOW, &fixture, &config).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert!(
        !bench.events().iter().any(|event| {
            event.payload().map(|payload| payload.kind_name())
                == Some("criterion_declared_unpassable")
        }),
        "a passing criterion, or one the suite holds, is never declared unpassable"
    );
}

#[tokio::test]
async fn a_criterion_no_work_on_the_plan_can_pass_is_declared_and_the_planner_is_routed() {
    let bench = Bench::new();
    let plan = plan_session(&format!(
        "tasks:\n{}",
        task_yaml("task-u", "u", "u.txt", "false")
    ));
    let fixture = plan
        + "  - match_prompt_contains: \"`task-u`\"
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
        arguments: { kind: unpassable, criterion: \"false\", why: \"false exits 1\" }
    outcome: { type: completed, summary: declared }
" + &tasks_session("tasks: []\n", "replanned");

    bench.run(UNPASSABLE_WORKFLOW, &fixture).await;

    assert_eq!(
        bench
            .events()
            .iter()
            .filter(|event| event.payload().map(|payload| payload.kind_name())
                == Some("criterion_declared_unpassable"))
            .count(),
        1
    );
    let opened = bench
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Session(SessionEvent::Opened(p)))
                    if p.task_id.as_ref().is_some_and(|id| id.as_str() == "task-u")
            )
        })
        .count();
    assert_eq!(opened, 1);
    // The planner declared the same criterion again, so the loop fails as
    // unchanged, with the declaration it repeats inside.
    assert!(loop_failure(&bench).is_some_and(|f| {
        serde_json::to_value(&f).unwrap()["unchanged"]["failure"]["unpassable"][0]["task_id"]
            == "task-u"
    }));
}

/// [`WORKFLOW`] with one reroute back to the planner, so a loop that fails
/// on a declared criterion gets its plan rewritten.
const UNPASSABLE_WORKFLOW: &str = r#"
name: departs
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "Write the tasks document."
    artifacts:
      produces: [tasks]
  - id: implement
    kind: loop
    runner: executor
    depends_on: [plan]
    until: all_tasks_complete
    prompt: "Implement your task."
    on_failure: { goto: plan, max_reroutes: 1 }
"#;
