//! What accepting a departure from a task's criterion does depends on
//! what holds the task to it. One the plan declares, and nothing else
//! supplies, stops holding the task: nothing else can rewrite it. The
//! suite the run measured holds every task, and no task departs from it.

mod common;

use common::*;
use yunta_core::events::{EventPayload, GateEvent, NodeEvent, Phase, TaskEvent, TaskStatus};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::{Bench, ScriptedInteraction, MOCK_CONFIG};

/// A plan whose one task must write a greeting that says hello.
const PLAN: &str = "\
tasks:
  - id: greet
    title: \"Write the greeting\"
    scope: [\"greeting.txt\"]
    criteria:
      - cmd: \"test -f greeting.txt\"
      - cmd: \"grep -q Hello greeting.txt\"
";

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

/// The task session: writes a greeting that says `Hi`, and declares the
/// departure from `from`.
fn departing(from: &str, expect: &str) -> String {
    plan_session(PLAN)
        + &format!(
            "  - match_prompt_contains: \"`greet`\"
    effects:
      - {{ path: greeting.txt, content: \"Hi\" }}
    steps:
      - type: run_tool
        tool: yunta_declare_deviation
{expect}        arguments:
          from: {from}
          planned: \"the greeting says Hello\"
          instead: \"it says Hi\"
          why: \"the brief asks for Hi\"
    outcome: {{ type: completed, summary: wrote-hi }}
"
        )
}

fn accepting() -> ScriptedInteraction {
    ScriptedInteraction::new(yunta_core::events::HumanChoice {
        option: "accept".into(),
        by: "lead".into(),
        free_text: None,
    })
}

#[tokio::test]
async fn a_criterion_of_the_plan_a_person_accepted_departing_from_no_longer_holds_its_task() {
    let bench = Bench::new();
    let RunReport { terminal, state } = bench
        .run_with_interaction(
            WORKFLOW,
            &departing("{ criterion: \"grep -q Hello greeting.txt\" }", ""),
            &accepting(),
        )
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
}

#[tokio::test]
async fn the_checks_after_the_answer_run_no_criterion_a_person_waived() {
    let bench = Bench::new();
    bench
        .run_with_interaction(
            WORKFLOW,
            &departing("{ criterion: \"grep -q Hello greeting.txt\" }", ""),
            &accepting(),
        )
        .await;

    let events = bench.events();
    let answered = events
        .iter()
        .position(|event| {
            matches!(
                event.payload(),
                Some(EventPayload::Tasks(TaskEvent::DeviationResolved(_)))
            )
        })
        .expect("a person answered the departure");
    let checked: Vec<String> = events[answered..]
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Node(NodeEvent::CriteriaChecked(p))) if p.phase == Phase::Post => {
                Some(p.results.iter().map(|r| r.cmd.clone()).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect();
    assert!(
        !checked.is_empty(),
        "the task's work was checked after the answer"
    );
    assert!(
        checked.iter().all(|cmd| cmd == "test -f greeting.txt"),
        "only the criterion still holding the task runs: {checked:?}"
    );
}

#[tokio::test]
async fn accepting_says_the_task_stops_being_held_to_that_criterion() {
    let bench = Bench::new();
    bench
        .run_with_interaction(
            WORKFLOW,
            &departing("{ criterion: \"grep -q Hello greeting.txt\" }", ""),
            &accepting(),
        )
        .await;

    let tradeoff = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(p))) => {
                Some(p.options()[0].tradeoff.clone())
            }
            _ => None,
        })
        .expect("the departure was put to a person");
    assert!(
        tradeoff.starts_with("The task stops being held to `grep -q Hello greeting.txt`"),
        "{tradeoff}"
    );
}

#[tokio::test]
async fn a_departure_from_the_suite_the_run_measured_is_refused() {
    let config = format!("{MOCK_CONFIG}baseline:\n  suite: \"true\"\n");
    // The work says Hello: the plan's criteria pass, and the suite is
    // the only thing the session departs from.
    let fixture = departing("{ criterion: \"true\" }", "        expect: refused\n")
        .replace("content: \"Hi\"", "content: \"Hello\"");
    let bench = Bench::new();
    let RunReport { terminal, state } = bench.run_with_config(WORKFLOW, &fixture, &config).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(state.tasks.status("greet"), Some(TaskStatus::Done));
    assert!(
        !bench.events().iter().any(|event| matches!(
            event.payload(),
            Some(EventPayload::Tasks(TaskEvent::DeviationDeclared(_)))
        )),
        "a departure from the suite never reaches the log"
    );
}
