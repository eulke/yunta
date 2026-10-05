//! Whether a round of questions stops a run is the workflow's to say. By
//! default every round that asks something waits for a person; a node or
//! a mode that says `ask: required` lets a round of optional questions —
//! each saying what it assumes — close in the same close, and the node
//! after it goes on with those assumptions. A required question waits.

use yunta_core::events::{Channel, EventPayload, GateEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

/// `grill` asks, then `brief` writes from the answers; `node` is the
/// `ask` line `grill` declares, and `mode` the one of the run's mode.
fn workflow(node: Option<&str>, mode: Option<&str>) -> String {
    let node = node
        .map(|ask| format!("    ask: {ask}\n"))
        .unwrap_or_default();
    let mode = mode.map(|ask| format!(", ask: {ask}")).unwrap_or_default();
    format!(
        r#"
name: ask
modes:
  only: {{ include: all{mode} }}
nodes:
  - id: grill
    kind: prompt
    runner: executor
    prompt: "Raise what you need to know."
{node}    artifacts:
      produces: [questions]
  - id: brief
    kind: prompt
    runner: executor
    depends_on: [grill]
    context:
      - artifact: {{ node: grill, kind: questions }}
      - artifact: {{ node: grill, kind: answers }}
    prompt: "Write the brief from the questions and their answers."
    artifacts:
      produces: [brief.md]
"#
    )
}

/// `grill` asks one question, `required` as given; `brief` writes its file.
fn fixture(staging: &std::path::Path, required: bool) -> String {
    format!(
        r##"
capabilities: {{ run_tools: true }}
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - id: q1
                text: "Which environment?"
                answer_type: choice
                values: [staging, production]
                required: {required}
                assumes: staging
    outcome: {{ type: completed, summary: "asked" }}
  - effects:
      - {{ path: {brief:?}, content: "# Brief\n" }}
    outcome: {{ type: completed, summary: "briefed" }}
"##,
        brief = staging.join("brief.md"),
    )
}

/// How the run's one round of questions was answered, and how it ended.
async fn asked(workflow: &str, required: bool) -> (RunTerminal, Option<Channel>) {
    let bench = Bench::new().in_mode("only");
    let fixture = fixture(&bench.staging("brief"), required);
    let RunReport { terminal, .. } = bench.run(workflow, &fixture).await;
    let channel = bench.events().iter().find_map(|e| match e.payload() {
        Some(EventPayload::Gates(GateEvent::QuestionsAnswered(p))) => Some(p.channel),
        _ => None,
    });
    (terminal, channel)
}

fn paused(terminal: &RunTerminal) -> bool {
    matches!(terminal, RunTerminal::Paused { .. })
}

#[tokio::test]
async fn an_optional_only_round_waits_by_default() {
    let (terminal, channel) = asked(&workflow(None, None), false).await;

    assert!(paused(&terminal), "{terminal:?}");
    assert_eq!(channel, None);
}

#[tokio::test]
async fn a_node_that_asks_only_required_lets_an_optional_round_go() {
    let (terminal, channel) = asked(&workflow(Some("required"), None), false).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(channel, Some(Channel::Assumed));
}

#[tokio::test]
async fn a_mode_that_asks_only_required_lets_optional_rounds_go() {
    let (terminal, channel) = asked(&workflow(None, Some("required")), false).await;

    assert_eq!(terminal, RunTerminal::Finished);
    assert_eq!(channel, Some(Channel::Assumed));
}

#[tokio::test]
async fn a_node_ask_beats_its_mode() {
    let workflow = workflow(Some("always"), Some("required"));

    let (terminal, channel) = asked(&workflow, false).await;

    assert!(paused(&terminal), "{terminal:?}");
    assert_eq!(channel, None);
}

#[tokio::test]
async fn a_required_question_waits_whatever_the_workflow_says() {
    let (terminal, channel) = asked(&workflow(Some("required"), None), true).await;

    assert!(paused(&terminal), "{terminal:?}");
    assert_eq!(channel, None);
}
