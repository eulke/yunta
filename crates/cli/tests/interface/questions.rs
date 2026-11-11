//! A round of questions driven end to end: a run this build parked on the questions its
//! node asked, answered in the application, and handed on to a detached resume this
//! build carries to its end with the answers on its log.

use super::*;

use yunta_testkit::{full_run_id, run_id_from, runs_root, wait_until, yunta_at, Checkout};

/// Every node runs on the mock adapter.
const CONFIG: &str = "defaults:\n  isolation: none\nrunners:\n  executor:\n    \
                      - { adapter: mock, model: mock-model }\n";

/// A node that asks: its session hands a round of questions over and ends.
const ASKS: &str = "name: asking\nnodes:\n  - id: ask\n    kind: prompt\n    runner: executor\n    \
                    prompt: \"Ask what has to be known.\"\n    artifacts:\n      produces: [questions]\n";

/// The session that hands one question over.
const HANDS_THEM_OVER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_questions
        arguments:
          document:
            questions:
              - { id: summary, text: "What changed?", answer_type: text, required: true }
    outcome: { type: completed, summary: asked }
"#;

/// A run parked on its round, answered in the application, goes on to its end with
/// the answer on its log.
#[tokio::test]
async fn a_parked_round_answered_is_resumed_with_its_answers() {
    let checkout = Checkout::new()
        .config(CONFIG)
        .workflow("wf", ASKS)
        .file("fixture.yaml", HANDS_THEM_OVER)
        .committed();
    let arguments = [
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ];
    let parked = yunta_at!(&checkout, &arguments);
    assert_eq!(parked.status.code(), Some(3), "the run parks on its round");
    let id = full_run_id(&checkout.home, &run_id_from(&parked));
    let performer = Arc::new(Performer::new(workspace(&checkout.repo, &checkout.home)));
    let (opened, asked) = State::opened(now());

    let mut answered = vec![Stroke::Enter, Stroke::Insert('a')];
    answered.extend("the greeting names the person".chars().map(Stroke::Insert));
    answered.push(Stroke::Enter);
    let (state, _) = drive(opened, asked, &performer, &answered).await;
    let screen = Markdown.screen(&state.screen(&Markdown.look()));

    let dir = runs_root(&checkout.home).join(&id);
    let log = || std::fs::read_to_string(dir.join("events.jsonl")).unwrap_or_default();
    wait_until(
        || log().contains("\"run_finished\""),
        || format!("the run never finished:\n{}\non screen:\n{screen}", log()),
    );
    let recorded = std::fs::read_to_string(dir.join("artifacts/ask/answers.yaml")).unwrap();
    assert!(
        recorded.contains("the greeting names the person"),
        "the answer is on the run:\n{recorded}"
    );
}
