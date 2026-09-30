//! A task session that cannot build what the plan declares says so, and
//! its task does not close on it until a person answers.

mod common;

use common::*;
use yunta_core::events::{
    DepartsFrom, EventPayload, GateEvent, SessionEvent, TaskEvent, TaskStatus,
};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction};

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
          from: {from}
          planned: \"Hello, and good night\"
          instead: \"Hello\"
          why: \"the second half needs the clock, which this task may not read\"
    outcome: {{ type: completed, summary: wrote-hello }}
"
    )
}

fn fixture(resumable: bool, then: &str) -> String {
    plan_session(PLAN).replace(
        "capabilities: { run_tools: true }",
        &format!("capabilities: {{ run_tools: true, resume_session: {resumable} }}"),
    ) + &departs("{ shape: Greeting }")
        + then
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

#[tokio::test]
async fn a_departure_accepted_closes_the_task_on_its_work_without_another_session() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(WORKFLOW, &fixture(true, ""), &answering("accept", None))
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
            &fixture(true, then),
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
    let RunReport { terminal, state } = bench.run(WORKFLOW, &fixture(true, "")).await;

    match terminal {
        RunTerminal::Paused { reason } => assert!(
            reason.contains("a departure from the plan needs a person's answer")
                && reason.contains("`greet`"),
            "{reason}"
        ),
        other => panic!("expected the run to pause, got {other:?}"),
    }
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Blocked));
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
