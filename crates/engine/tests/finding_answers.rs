//! A node answers a finding another node reported — its work fixed it,
//! or it declines to — and says why. What it cannot answer is refused,
//! and an answer the engine takes stands on the log beside the finding.

mod common;

use yunta_core::events::{EventPayload, FindingAnswer, FindingEvent};
use yunta_engine::RunReport;
use yunta_testkit::Bench;

/// A review that finds two things, then takes one back; a fix that
/// answers; a reader of what happened to the run's findings.
const WORKFLOW: &str = r#"
name: answered
nodes:
  - id: review
    kind: prompt
    runner: executor
    prompt: "Review the change."
    artifacts:
      produces: [findings]
  - id: fix
    kind: prompt
    runner: executor
    depends_on: [review]
    prompt: "Fix the findings."
  - id: read
    kind: prompt
    runner: executor
    depends_on: [fix]
    context:
      - run-events: { filter: findings }
    prompt: "Read the findings."
"#;

/// A call to the answer tool, refused unless `accepted`.
fn answering(node: &str, id: &str, why: &str, accepted: bool) -> String {
    let expect = if accepted {
        ""
    } else {
        "        expect: refused\n"
    };
    format!(
        "      - type: run_tool\n        tool: yunta_answer_finding\n{expect}\
         \x20       arguments: {{ node: {node}, id: {id}, answer: fixed, why: {why:?} }}\n"
    )
}

fn fixture() -> String {
    let posting = |id: &str, severity: &str| {
        format!(
            "      - type: run_tool\n        tool: yunta_post_finding\n        arguments:\n\
             \x20         id: {id}\n          severity: {severity}\n          title: \"a bug\"\n\
             \x20         location: \"src/lib.rs:1\"\n          detail: \"it breaks\"\n"
        )
    };
    format!(
        "capabilities: {{ run_tools: true }}
sessions:
  - match_prompt_contains: \"Review the change\"
    steps:
{}{}      - type: run_tool
        tool: yunta_withdraw_finding
        arguments: {{ id: gone, reason: \"not a bug\" }}
    outcome: {{ type: completed, summary: reviewed }}
  - match_prompt_contains: \"Fix the findings\"
    steps:
{}{}{}{}{}{}    outcome: {{ type: completed, summary: answered }}
  - match_prompt_contains: \"Read the findings\"
    outcome: {{ type: completed, summary: read }}
",
        posting("bug", "blocking"),
        posting("gone", "blocking"),
        posting("own", "minor"),
        answering("review", "nothing", "no such finding", false),
        answering("review", "gone", "it was taken back", false),
        answering("fix", "own", "its own finding", false),
        answering("review", "bug", " ", false),
        answering("review", "bug", "the check is in", true),
    )
}

/// What the run's log says each node answered.
fn answers(bench: &Bench) -> Vec<(String, String, FindingAnswer, String)> {
    bench
        .events()
        .iter()
        .filter_map(|event| match event.payload() {
            Some(EventPayload::Findings(FindingEvent::Answered(p))) => Some((
                event
                    .node_id
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                format!("{}/{}", p.node, p.id),
                p.answer,
                p.why.clone(),
            )),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_answer_names_a_finding_another_node_reported_and_stands() {
    let bench = Bench::new();
    let RunReport { terminal, .. } = bench.run(WORKFLOW, &fixture()).await;

    assert_eq!(terminal, yunta_engine::RunTerminal::Finished);
    assert_eq!(
        answers(&bench),
        [(
            "fix".to_string(),
            "review/bug".to_string(),
            FindingAnswer::Fixed,
            "the check is in".to_string()
        )],
        "an unknown, withdrawn or own finding, and an answer without why, are refused"
    );
}

#[tokio::test]
async fn the_history_a_node_reads_carries_the_answers() {
    let bench = Bench::new();
    bench.run(WORKFLOW, &fixture()).await;

    let read = bench
        .mock()
        .requests_seen()
        .into_iter()
        .find(|request| request.prompt.contains("Read the findings"))
        .expect("the reader ran");
    assert!(read.prompt.contains("finding_answered"), "{}", read.prompt);
}

#[tokio::test]
async fn an_answered_blocking_finding_still_keeps_the_run_from_success() {
    let bench = Bench::new();
    bench.run(WORKFLOW, &fixture()).await;

    let workflow: yunta_core::Workflow = yunta_core::yaml::parse(WORKFLOW).unwrap();
    let events = bench.events();
    let at = events.last().unwrap().timestamp;
    let frame = yunta_engine::run_frame(&bench.run_id, &workflow, &events, None, at);
    assert_eq!(
        frame.blocking_findings, 1,
        "a node's answer is its word, not evidence: the finding it answered still stands"
    );
}
