//! A run stopped by the token cap, from any interface: what it offers, and what each
//! answer does with no terminal attached to the engine that goes on.

use yunta_testkit::{full_run_id, runs_root, unwrapped, wait_until};

use super::*;

/// The log of the run `id`, as the engine last exported it.
pub(super) fn log_of(home: &Path, id: &str) -> String {
    std::fs::read_to_string(runs_root(home).join(id).join("events.jsonl")).unwrap_or_default()
}

/// How many events of `kind` a log holds, and not how many times the word appears: a
/// pause carries the kind of what it stopped on.
pub(super) fn count(log: &str, kind: &str) -> usize {
    log.matches(&format!("\"kind\":\"{kind}\"")).count()
}

/// Kills the engine the registry of the run `id` names, so it says nothing more, and waits
/// for it to be gone.
fn kills_the_engine_of(home: &Path, id: &str) {
    let registry = runs_root(home).join(id).join("scratch").join("engine.json");
    let engine: serde_json::Value =
        serde_json::from_slice(&std::fs::read(registry).unwrap()).unwrap();
    let pid = engine["engine_pid"].to_string();
    let signal = |args: &[&str]| {
        std::process::Command::new("kill")
            .args(args)
            .status()
            .unwrap()
    };
    assert!(signal(&["-9", &pid]).success());
    wait_until(
        || !signal(&["-0", &pid]).success(),
        || "the killed engine is still there".into(),
    );
}

/// How many sessions the run `id` has opened, read from the store because the exported
/// log waits for a pause.
fn sessions_opened(home: &Path, id: &str) -> usize {
    yunta_storage::Storage::open(&home.join("yunta.db"))
        .unwrap()
        .events_for_run(&yunta_core::RunId::from(id))
        .unwrap()
        .iter()
        .filter(|event| {
            matches!(
                event.payload(),
                Some(yunta_core::events::EventPayload::Session(
                    yunta_core::events::SessionEvent::Opened(_)
                ))
            )
        })
        .count()
}

/// Waits until the log of the run `id` holds `times` events of `kind`.
pub(super) fn waits_for_event(home: &Path, id: &str, kind: &str, times: usize) {
    wait_until(
        || count(&log_of(home, id), kind) >= times,
        || {
            format!(
                "the log never held {times} of `{kind}`: {}",
                log_of(home, id)
            )
        },
    );
}

/// The page offers what stops a person from guessing: the spend, each option with what
/// it does, and the command that chooses it.
#[test]
fn a_run_stopped_on_the_cap_shows_its_decision() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget(root.path());
    let run = handle(&run_id);

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    let said = unwrapped(&stdout(&status));
    for words in [
        "its token budget is spent (100 tokens with `limits.max_tokens_per_run: 100`).".to_string(),
        "decision on the run".to_string(),
        "limits.max_tokens_per_run".to_string(),
        "continue: Continue past the cap".to_string(),
        format!("yunta resolve-gate {run} continue"),
        "abort: Pause here".to_string(),
        format!("yunta resolve-gate {run} abort"),
    ] {
        assert!(said.contains(&words), "no `{words}` in: {said}");
    }
    assert!(
        !said.contains("no option answers this pause here"),
        "{said}"
    );

    let listed = yunta_in!(&repo, &home, &["list", "--runs"]);
    let said = unwrapped(&stdout(&listed));
    let words = format!("yunta status {run}");
    assert!(said.contains(&words), "no `{words}` in: {said}");
}

/// The decision a program reads belongs to the run, so it has no node, and its commands
/// name the run whole, which is what a program copies.
#[test]
fn a_program_reads_the_run_s_decision_with_no_node() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget(root.path());
    let id = full_run_id(&home, &run_id);

    let status = yunta_in!(&repo, &home, &["status", &id, "--json"]);
    let document: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    let decision = &document["decision"];
    assert!(decision.get("node").is_none(), "{document:#}");
    assert_eq!(decision["budget"]["cap"], 100, "{document:#}");
    assert_eq!(decision["budget"]["spent"], 100, "{document:#}");
    let options: Vec<&str> = decision["options"]
        .as_array()
        .expect("the decision's options")
        .iter()
        .filter_map(|option| option["id"].as_str())
        .collect();
    assert_eq!(options, ["continue", "abort"], "{document:#}");
    assert_eq!(
        decision["commands"]["continue"],
        format!("yunta resolve-gate {id} continue"),
        "{document:#}"
    );
    assert!(document.get("waiting_on").is_some(), "{document:#}");
}

/// With nothing attached to it: the answer is recorded, a resume in the background takes
/// it, and the node the cap stopped starts.
#[test]
fn continue_from_another_process_goes_past_the_cap() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget(root.path());
    let id = full_run_id(&home, &run_id);

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &id, "continue"]);
    assert!(answered.status.success(), "{}", stderr(&answered));

    waits_for_event(&home, &id, "run_finished", 1);
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "node_started"), 4, "the node left ran: {log}");
    assert_eq!(count(&log, "gate_resolved"), 1, "{log}");
    assert_eq!(count(&log, "run_resumed"), 1, "{log}");
}

/// `abort` is an answer too, and it is the one a person takes back: the run is where it
/// was, spending nothing, and the same decision is on offer.
#[test]
fn abort_leaves_the_run_on_the_cap_with_its_decision_open() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget(root.path());
    let id = full_run_id(&home, &run_id);

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &id, "abort"]);
    assert!(answered.status.success(), "{}", stderr(&answered));

    waits_for_event(&home, &id, "run_paused", 2);
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "node_started"), 3, "nothing ran: {log}");
    let status = yunta_in!(&repo, &home, &["status", &id]);
    let said = unwrapped(&stdout(&status));
    assert!(said.contains("decision on the run"), "{said}");
}

/// The chain of [`OVER_BUDGET`], whose last node, past the cap, holds until `go` exists or
/// its marker is gone with the test, and says it started.
fn chain_up_to_a_hold(started: &Path, go: &Path) -> String {
    format!(
        "name: over-budget\nnodes:\n  \
         - {{ id: first, kind: prompt, runner: executor, prompt: \"Spend.\" }}\n  \
         - {{ id: second, kind: prompt, runner: executor, depends_on: [first], prompt: \"Spend.\" }}\n  \
         - {{ id: third, kind: prompt, runner: executor, depends_on: [second], prompt: \"Spend.\" }}\n  \
         - id: hold\n    kind: bash\n    depends_on: [third]\n    \
         run: \"echo in >> {0}; until [ -f {1} ] || [ ! -e {0} ]; do sleep 0.05; done\"\n",
        started.display(),
        go.display(),
    )
}

/// Every engine spends new money, so an answer pays for the one that took it: one killed
/// mid-node, which never got to say anything, leaves the next one stopped on the cap.
///
/// Reached by: `crates/replay/src/decisions.rs`, `crates/engine/src/run/budget.rs`
#[test]
fn an_engine_killed_after_it_took_the_answer_leaves_the_next_on_the_cap() {
    let root = tempfile::tempdir().unwrap();
    let (started, go) = (root.path().join("started"), root.path().join("go"));
    let workflow = chain_up_to_a_hold(&started, &go);
    let (repo, home, run_id) = parked_over_budget_on(root.path(), &workflow, OVER_BUDGET_FIXTURE);
    let id = full_run_id(&home, &run_id);

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &id, "continue"]);
    assert!(answered.status.success(), "{}", stderr(&answered));
    wait_until(
        || started.exists(),
        || "the engine that took the answer never reached the node".into(),
    );

    kills_the_engine_of(&home, &id);
    std::fs::write(&go, "go").unwrap();

    let next = yunta_in!(&repo, &home, &["resume", &id]);
    assert_eq!(next.status.code(), Some(3), "{}", stderr(&next));
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "gate_resolved"), 1, "{log}");
    assert_eq!(
        std::fs::read_to_string(&started).unwrap().lines().count(),
        1,
        "the node did not start again: {log}"
    );
    assert!(
        log.contains("\"budget_exhausted\""),
        "it stopped on the cap: {log}"
    );
}

/// Sessions of [`OVER_BUDGET`] that go past their share of the cap, as a real session does:
/// the first is cut before the run has spent its cap.
const PAST_THEIR_SHARE: &str = r#"
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

/// The next attempt would get the same share, so `retry` would fail the same way until
/// the cap is spent. What moves the run is going past the cap, and the menu offers that.
#[test]
fn a_cut_session_offers_continue_and_refuses_retry() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget_on(root.path(), OVER_BUDGET, PAST_THEIR_SHARE);
    let id = full_run_id(&home, &run_id);
    let run = handle(&id);

    let status = yunta_in!(&repo, &home, &["status", &id]);
    let said = unwrapped(&stdout(&status));
    for words in [
        "exceeded max_tokens".to_string(),
        "decision on node `first`".to_string(),
        "continue: Run it again past the cap (attempt 2)".to_string(),
        "Lifts the cap for this invocation only".to_string(),
        format!("yunta resolve-gate {run} continue"),
        "abort: Pause here".to_string(),
    ] {
        assert!(said.contains(&words), "no `{words}` in: {said}");
    }
    assert!(!said.contains("retry:"), "{said}");

    let status = yunta_in!(&repo, &home, &["status", &id, "--json"]);
    let document: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    let remedies = &document["decision"]["failure"]["remedies"];
    assert_eq!(
        remedies,
        &serde_json::json!([{"option": "continue", "precondition": null}]),
        "{document:#}"
    );

    let refused = yunta_in!(&repo, &home, &["resolve-gate", &id, "retry"]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    let said = unwrapped(&stderr(&refused));
    let words = "option `retry` isn't valid here; declared options: continue, abort";
    assert!(said.contains(words), "no `{words}` in: {said}");
}

/// Its next attempt spends past its share as the first did, so with the cap still on it
/// this is cut the same way: only `continue` lifting it lets the run go on.
///
/// Reached by: `crates/engine/src/run/budget.rs`, `crates/engine/src/run/steps.rs`
#[test]
fn continue_runs_a_cut_session_past_its_share() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget_on(root.path(), OVER_BUDGET, PAST_THEIR_SHARE);
    let id = full_run_id(&home, &run_id);

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &id, "continue"]);
    assert!(answered.status.success(), "{}", stderr(&answered));

    waits_for_event(&home, &id, "run_finished", 1);
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "node_failed"), 1, "cut once: {log}");
    assert_eq!(count(&log, "gate_resolved"), 1, "one answer: {log}");
    assert_eq!(count(&log, "run_resumed"), 1, "{log}");
}

/// [`PAST_THEIR_SHARE`], with its second session, the attempt `continue` starts, still at
/// work when the test kills its engine.
const CUT_THEN_HELD: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 5, output_tokens: 0, after_ms: 60000 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 60, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
"#;

/// The lift lives in the engine that took the answer: one killed mid-attempt leaves the
/// next under its share, and it asks again when it is cut.
///
/// Reached by: `crates/replay/src/decisions.rs`
#[test]
fn an_engine_killed_mid_attempt_leaves_the_next_under_its_share() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget_on(root.path(), OVER_BUDGET, CUT_THEN_HELD);
    let id = full_run_id(&home, &run_id);

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &id, "continue"]);
    assert!(answered.status.success(), "{}", stderr(&answered));

    // Its session opened, so the held script is the one it took: killed any sooner, the
    // next engine would be the one to play it.
    wait_until(
        || sessions_opened(&home, &id) == 2,
        || "the engine that took the answer never opened the attempt's session".into(),
    );

    kills_the_engine_of(&home, &id);

    let next = yunta_in!(&repo, &home, &["resume", &id]);
    assert_eq!(next.status.code(), Some(3), "{}", stderr(&next));
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "node_finished"), 0, "cut again: {log}");
    let status = yunta_in!(&repo, &home, &["status", &id]);
    let said = unwrapped(&stdout(&status));
    let words = "continue: Run it again past the cap";
    assert!(said.contains(words), "it asks again: {said}");
}

/// Two nodes of a chain whose second session spends what is left of the cap of 100 and
/// dies, a failure the cap did not cause.
const FAILS_AT_THE_CAP: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - { type: usage, input_tokens: 50, output_tokens: 0 }
    outcome: { type: completed, summary: spent }
  - steps:
      - { type: usage, input_tokens: 50, output_tokens: 0 }
    outcome: { type: crash }
  - outcome: { type: completed, summary: spent }
"#;

const TWO_NODES: &str = r#"
name: over-budget
nodes:
  - { id: first, kind: prompt, runner: executor, prompt: "Spend." }
  - { id: second, kind: prompt, runner: executor, depends_on: [first], prompt: "Spend." }
"#;

/// With what a person answered standing, they answer the cap once and the run moves on,
/// and while it stands on the cap the cap is what the page asks about.
#[test]
fn a_retry_that_reached_the_cap_moves_on_with_one_continue() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run_id) = parked_over_budget_on(root.path(), TWO_NODES, FAILS_AT_THE_CAP);
    let id = full_run_id(&home, &run_id);

    let retried = yunta_in!(&repo, &home, &["resolve-gate", &id, "retry"]);
    assert!(retried.status.success(), "{}", stderr(&retried));
    waits_for_event(&home, &id, "run_paused", 2);
    let status = yunta_in!(&repo, &home, &["status", &id]);
    let said = unwrapped(&stdout(&status));
    assert!(said.contains("decision on the run"), "{said}");

    let continued = yunta_in!(&repo, &home, &["resolve-gate", &id, "continue"]);
    assert!(continued.status.success(), "{}", stderr(&continued));
    waits_for_event(&home, &id, "run_finished", 1);
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "gate_resolved"), 2, "{log}");
    assert_eq!(count(&log, "node_started"), 3, "{log}");
}
