//! Only a required question stops a run: a node that asks only what the
//! work can do without — each question saying what it assumes — closes in
//! the same close, and the node after it goes on with those assumptions.

use yunta_core::events::{Channel, EventPayload, GateEvent};
use yunta_engine::{RunReport, RunTerminal};
use yunta_testkit::Bench;

const ASK_THEN_BRIEF: &str = r#"
name: ask
nodes:
  - id: grill
    kind: prompt
    runner: executor
    prompt: "Raise what you need to know."
    artifacts:
      produces: [questions]
  - id: brief
    kind: prompt
    runner: executor
    depends_on: [grill]
    context:
      - artifact: { node: grill, kind: questions }
      - artifact: { node: grill, kind: answers }
    prompt: "Write the brief from the questions and their answers."
    artifacts:
      produces: [brief.md]
"#;

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

fn channel(bench: &Bench) -> Option<Channel> {
    bench.events().iter().find_map(|e| match e.payload() {
        Some(EventPayload::Gates(GateEvent::QuestionsAnswered(p))) => Some(p.channel),
        _ => None,
    })
}

#[tokio::test]
async fn optional_only_round_does_not_pause() {
    let bench = Bench::new();
    let fixture = fixture(&bench.staging("brief"), false);

    let RunReport { terminal, .. } = bench.run(ASK_THEN_BRIEF, &fixture).await;

    assert_eq!(terminal, RunTerminal::Finished);
    let asked = bench.events().iter().any(|e| {
        matches!(
            e.payload(),
            Some(EventPayload::Gates(GateEvent::QuestionsAsked(_)))
        )
    });
    assert!(asked, "the questions are still on the log");
    assert_eq!(channel(&bench), Some(Channel::Assumed));
}

#[tokio::test]
async fn required_still_pauses() {
    let bench = Bench::new();
    let fixture = fixture(&bench.staging("brief"), true);

    let RunReport { terminal, .. } = bench.run(ASK_THEN_BRIEF, &fixture).await;

    assert!(
        matches!(terminal, RunTerminal::Paused { .. }),
        "{terminal:?}"
    );
    assert_eq!(channel(&bench), None);
}
