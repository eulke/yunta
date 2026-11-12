//! A round of questions driven end to end: a run this build parked on the questions its
//! node asked, answered in the application, and handed on to a detached resume this
//! build carries to its end with the answers on its log.

use super::*;

use yunta_testkit::workflows::{ASKS, EXECUTOR_ON_THE_MOCK, HANDS_ONE_QUESTION_OVER};
use yunta_testkit::{full_run_id, run_id_from, runs_root, wait_until, yunta_at, Checkout};

/// A run parked on its round, answered in the application, goes on to its end with
/// the answer on its log.
#[tokio::test]
async fn a_parked_round_answered_is_resumed_with_its_answers() {
    let checkout = Checkout::new()
        .config(EXECUTOR_ON_THE_MOCK)
        .workflow("wf", ASKS)
        .file("fixture.yaml", HANDS_ONE_QUESTION_OVER)
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
