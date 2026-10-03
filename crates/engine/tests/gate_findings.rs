//! A gate that shows the run's findings shows every finding standing in
//! it — each reviewer's, those no document holds — each with the node
//! that found it and the answers other nodes gave it, pinned to the
//! question by the hash of what it showed. A person who goes on past it
//! settles every finding it showed; one who sends the run back or stops
//! settles nothing.

mod common;

use common::*;
use yunta_core::events::{EventPayload, FindingAnswer, GateEvent};
use yunta_engine::{RunReport, RunTerminal, ShownContent};
use yunta_testkit::Bench;

/// Two reviewers, a fix that answers, and a gate showing the run's
/// findings.
const WORKFLOW: &str = r#"
name: shown
nodes:
  - id: review
    kind: prompt
    runners: [planner, executor]
    prompt: "Review the change."
    artifacts:
      produces: [findings]
  - id: fix
    kind: prompt
    runner: executor
    depends_on: [review]
    prompt: "Fix the findings."
  - id: ship
    kind: gate
    depends_on: [fix]
    assignee: lead
    message: "Ship it?"
    shows: [{ kind: findings }]
"#;

fn posting(id: &str, title: &str) -> String {
    format!(
        "      - type: run_tool\n        tool: yunta_post_finding\n        arguments:\n\
         \x20         id: {id}\n          severity: blocking\n          title: {title:?}\n\
         \x20         location: \"src/lib.rs:1\"\n          detail: \"it breaks\"\n"
    )
}

/// Each reviewer posts its own `f-1`; the fix posts one of its own —
/// no document of its holds it — and answers the first reviewer's.
fn fixture() -> String {
    format!(
        "capabilities: {{ run_tools: true }}
sessions:
  - steps:
{}    outcome: {{ type: completed, summary: reviewed }}
  - steps:
{}    outcome: {{ type: completed, summary: reviewed }}
  - steps:
{}      - type: run_tool
        tool: yunta_answer_finding
        arguments: {{ node: review@planner, id: f-1, answer: fixed, why: \"the check is in\" }}
    outcome: {{ type: completed, summary: fixed }}
",
        posting("f-1", "the planner's bug"),
        posting("f-1", "the executor's bug"),
        posting("left", "something the fix left"),
    )
}

/// The run's findings the gate showed when it asked.
fn shown_findings(interaction: &SequencedInteraction) -> yunta_core::events::findings::RunFindings {
    let shown = interaction.shown();
    let ShownContent::RunFindings(view) = &shown[0][0].content else {
        panic!("the gate shows the run's findings: {shown:?}");
    };
    view.clone()
}

#[tokio::test]
async fn a_gate_that_shows_the_runs_findings_shows_each_reviewers_and_those_no_document_holds() {
    let interaction = SequencedInteraction::choosing(&["approve"]);
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench
        .run_with_interaction(WORKFLOW, &fixture(), &interaction)
        .await;

    assert_eq!(terminal, RunTerminal::Finished);
    let found: Vec<(Option<String>, String)> = shown_findings(&interaction)
        .findings
        .iter()
        .map(|standing| {
            (
                standing.node.as_ref().map(ToString::to_string),
                standing.finding.title.clone(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (
                Some("review@planner".to_string()),
                "the planner's bug".to_string()
            ),
            (
                Some("review@executor".to_string()),
                "the executor's bug".to_string()
            ),
            (
                Some("fix".to_string()),
                "something the fix left".to_string()
            ),
        ]
    );
}

#[tokio::test]
async fn a_gate_shows_each_finding_with_the_answer_another_node_gave_it() {
    let interaction = SequencedInteraction::choosing(&["approve"]);
    let bench = Bench::new();
    bench
        .run_with_interaction(WORKFLOW, &fixture(), &interaction)
        .await;

    let view = shown_findings(&interaction);
    let answered = &view.findings[0].answers;
    assert_eq!(answered.len(), 1, "{view:?}");
    assert_eq!(answered[0].by.as_ref().unwrap().as_str(), "fix");
    assert_eq!(answered[0].answer, FindingAnswer::Fixed);
    assert!(view.findings[1].answers.is_empty());
}

#[tokio::test]
async fn what_the_gate_showed_is_kept_by_its_hash() {
    let interaction = SequencedInteraction::choosing(&["approve"]);
    let bench = Bench::new();
    bench
        .run_with_interaction(WORKFLOW, &fixture(), &interaction)
        .await;

    let pinned = bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(p))) => Some(p.shows()[0].clone()),
            _ => None,
        })
        .expect("the gate asked");
    assert!(pinned.producer.is_none(), "the run's own view: {pinned:?}");
    let shown = &interaction.shown()[0][0];
    assert_eq!(
        shown.shown, pinned,
        "a person reads exactly what the log pins"
    );
    let bytes = tokio::fs::read(&shown.path).await.expect("the view's file");
    assert_eq!(yunta_core::ContentHash::sha256(&bytes), pinned.content_hash);
}

#[tokio::test]
async fn a_parked_gate_answered_elsewhere_names_the_same_findings() {
    let bench = parked(WORKFLOW, &fixture()).await;

    answer_parked(&bench, "approve")
        .await
        .expect("the escalation rebuilt from the log takes the answer");
    let RunReport { terminal, .. } = bench.wake().await;

    assert_eq!(terminal, RunTerminal::Finished);
}

/// Every finding a person settled, as `node/id`.
fn settled(bench: &Bench) -> Vec<String> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(yunta_core::events::FindingEvent::Settled(p))) => {
                Some(format!(
                    "{}/{}",
                    p.node.as_ref().map(ToString::to_string).unwrap_or_default(),
                    p.id
                ))
            }
            _ => None,
        })
        .collect()
}

fn blocking(bench: &Bench, workflow: &str) -> usize {
    let workflow: yunta_core::Workflow = yunta_core::yaml::parse(workflow).unwrap();
    let events = bench.events();
    let at = events.last().unwrap().timestamp;
    yunta_engine::run_frame(&bench.run_id, &workflow, &events, None, at).blocking_findings
}

#[tokio::test]
async fn going_on_settles_each_finding_shown_and_the_run_succeeds() {
    let interaction = SequencedInteraction::choosing(&["approve"]);
    let bench = Bench::new();
    bench
        .run_with_interaction(WORKFLOW, &fixture(), &interaction)
        .await;

    assert_eq!(
        settled(&bench),
        ["review@planner/f-1", "review@executor/f-1", "fix/left"]
    );
    assert_eq!(blocking(&bench, WORKFLOW), 0);
    let tradeoff = &bench
        .events()
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Gates(GateEvent::Waiting(p))) => {
                Some(p.options()[0].tradeoff.clone())
            }
            _ => None,
        })
        .unwrap();
    assert!(
        tradeoff.contains("settles every finding it shows"),
        "{tradeoff}"
    );
}

#[tokio::test]
async fn aborting_settles_nothing() {
    let bench = Bench::new();
    bench
        .run_with_interaction(
            WORKFLOW,
            &fixture(),
            &SequencedInteraction::choosing(&["abort"]),
        )
        .await;

    assert!(settled(&bench).is_empty(), "{:?}", settled(&bench));
    assert_eq!(
        blocking(&bench, WORKFLOW),
        3,
        "each reviewer's and the fix's own"
    );
}

#[tokio::test]
async fn an_option_that_sends_the_run_back_settles_nothing() {
    let sends_back = WORKFLOW.replace(
        "    shows: [{ kind: findings }]",
        "    options: [approve, again]\n    on: { again: fix }\n    shows: [{ kind: findings }]",
    );
    let fixture = fixture() + "  - outcome: { type: completed, summary: looked again }\n";
    let interaction =
        SequencedInteraction::answering(vec![("again", Some("look again")), ("abort", None)]);
    let bench = Bench::new();
    bench
        .run_with_interaction(&sends_back, &fixture, &interaction)
        .await;

    assert!(settled(&bench).is_empty(), "{:?}", settled(&bench));
}

#[tokio::test]
async fn a_decision_recorded_while_parked_settles_what_the_gate_showed() {
    let bench = parked(WORKFLOW, &fixture()).await;

    answer_parked(&bench, "approve").await.unwrap();
    bench.wake().await;

    assert_eq!(settled(&bench).len(), 3, "{:?}", settled(&bench));
}
