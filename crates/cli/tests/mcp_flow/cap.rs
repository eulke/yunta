//! A run stopped by the token cap, from a real client: it reads what a program reads off
//! the command line, and its answer leaves the log a person's answer leaves.

use std::path::Path;

use yunta_testkit::{full_run_id, runs_root, Checkout, MOCK_CONFIG};

use super::*;

/// Four sessions in a chain under a cap of 100, so each may spend 25, 33, 50 and 100.
const CHAIN: &str = r#"
name: over-budget
nodes:
  - { id: first, kind: prompt, runner: executor, prompt: "One." }
  - { id: second, kind: prompt, runner: executor, depends_on: [first], prompt: "Two." }
  - { id: third, kind: prompt, runner: executor, depends_on: [second], prompt: "Three." }
  - { id: more, kind: prompt, runner: executor, depends_on: [third], prompt: "Four." }
"#;

/// Each of the first three sessions spends what it may: the cap is spent with `more` left,
/// so the run itself stops on it.
const ON_THE_CAP: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 25, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 33, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 42, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 10, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
"#;

/// The first session spends past its 25, as a real one does, and is cut. What follows
/// finishes the chain once the cap is lifted.
const CUT_AT_ITS_SHARE: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
"#;

/// The two ways a run meets the cap, by the fixture that brings each.
const STOPS: [(&str, &str); 2] = [("on the cap", ON_THE_CAP), ("cut", CUT_AT_ITS_SHARE)];

/// A run of [`CHAIN`] stopped by `fixture`, and its whole id.
fn stopped(root: &Path, fixture: &str) -> (Checkout, String) {
    let checkout = Checkout::under(root)
        .config(&format!(
            "{MOCK_CONFIG}limits:\n  max_tokens_per_run: 100\n"
        ))
        .workflow("over-budget", CHAIN)
        .file("fixture.yaml", fixture)
        .committed();
    let run = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &[
            "run",
            "over-budget.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    );
    let id = full_run_id(&checkout.home, &run_id_from(&run));
    (checkout, id)
}

/// A document without what moves with the clock between two reads.
fn at_rest(mut document: Value) -> Value {
    let fields = document.as_object_mut().expect("a document");
    fields.remove("elapsed_secs");
    fields.remove("time");
    document
}

/// The events the run `id` logged up to its finish, as this test reads them: the kind of
/// each, and what an answer recorded.
async fn finished_log(checkout: &Checkout, id: &str) -> (Vec<String>, Vec<Value>) {
    let path = runs_root(&checkout.home).join(id).join("events.jsonl");
    let log = wait_for_async(
        || {
            let path = path.clone();
            async move {
                let log = std::fs::read_to_string(&path).unwrap_or_default();
                log.contains("\"kind\":\"run_finished\"").then_some(log)
            }
        },
        || format!("the run {id} never finished"),
    )
    .await;
    let events: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let kinds = events
        .iter()
        .map(|event| event["kind"].as_str().unwrap().to_string())
        .collect();
    let answers = events
        .iter()
        .filter(|event| event["kind"] == "gate_resolved")
        .map(|event| event["payload"].clone())
        .collect();
    (kinds, answers)
}

/// What an agent reads to decide is what a program reads, on both stops, all of it.
#[tokio::test]
async fn workflow_status_shows_what_status_json_shows_on_both_stops() {
    for (stop, fixture) in STOPS {
        let root = tempfile::tempdir().unwrap();
        let (checkout, id) = stopped(root.path(), fixture);

        let status = yunta_in!(&checkout.repo, &checkout.home, &["status", &id, "--json"]);
        let document: Value = serde_json::from_slice(&status.stdout).unwrap();
        let options = &document["decision"]["options"];
        assert!(
            options.as_array().is_some_and(|all| !all.is_empty()),
            "{stop}: {document:#}"
        );

        let read = called(
            &checkout.repo,
            &checkout.home,
            "workflow_status",
            json!({"run_id": id}),
        )
        .await;
        let read: Value = serde_json::from_str(&tool_text(&read)).unwrap();
        assert_eq!(at_rest(read), at_rest(document), "{stop}");
    }
}

/// Whichever door the answer came by, the run goes past the cap in an engine of its own,
/// and the logs they leave cannot be told apart.
#[tokio::test]
async fn continue_over_mcp_leaves_the_log_the_command_leaves() {
    for (stop, fixture) in STOPS {
        let by_hand = tempfile::tempdir().unwrap();
        let (typed, typed_id) = stopped(by_hand.path(), fixture);
        let answered = yunta_in!(
            &typed.repo,
            &typed.home,
            &["resolve-gate", &typed_id, "continue"]
        );
        assert!(answered.status.success(), "{stop}: {}", stderr(&answered));

        let by_agent = tempfile::tempdir().unwrap();
        let (agent, agent_id) = stopped(by_agent.path(), fixture);
        let answered = called(
            &agent.repo,
            &agent.home,
            "resolve_gate",
            json!({"run_id": agent_id, "option": "continue"}),
        )
        .await;
        assert_ne!(answered.is_error, Some(true), "{stop}: {answered:#?}");
        let said = tool_text(&answered);
        let words = format!("run {agent_id}: resolved `continue`");
        assert!(said.contains(&words), "{stop}: {said}");

        let (kinds, answers) = finished_log(&typed, &typed_id).await;
        let (agent_kinds, agent_answers) = finished_log(&agent, &agent_id).await;
        assert_eq!(agent_kinds, kinds, "{stop}");
        assert_eq!(agent_answers, answers, "{stop}");
    }
}

/// What the menu leaves out is left out over MCP too, in the sentence a person reads, and
/// the run is where it was.
#[tokio::test]
async fn retry_on_a_cut_session_is_refused_over_mcp_as_at_the_command() {
    let root = tempfile::tempdir().unwrap();
    let (checkout, id) = stopped(root.path(), CUT_AT_ITS_SHARE);

    let refused = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["resolve-gate", &id, "retry"]
    );
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = stderr(&refused);
    let sentence = said.trim_end().strip_prefix("error: ").unwrap_or(&said);

    let answered = called(
        &checkout.repo,
        &checkout.home,
        "resolve_gate",
        json!({"run_id": id, "option": "retry"}),
    )
    .await;
    assert_eq!(answered.is_error, Some(true), "{answered:#?}");
    assert_eq!(
        unwrapped(&tool_text(&answered)),
        unwrapped(&sentence.replace(yunta_testkit::handle(&id), &id))
    );

    let log =
        std::fs::read_to_string(runs_root(&checkout.home).join(&id).join("events.jsonl")).unwrap();
    assert!(!log.contains("\"kind\":\"gate_resolved\""), "{log}");
}
