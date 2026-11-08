//! A parent parked on a child the token cap stopped: it hands its reader the way there,
//! the child is answered where it stopped, and each cap stays its own.

use yunta_testkit::{full_run_id, unwrapped};

use super::cap::{count, log_of, waits_for_event};
use super::*;

/// A parent that composes the chain of [`OVER_BUDGET`], with its child stopped on its cap,
/// and `after` for what the parent does once the child is done.
fn parent_of_a_child_on_the_cap(root: &Path, after: &str) -> (PathBuf, PathBuf, String) {
    let parent =
        format!("name: outer\nnodes:\n  - {{ id: compose, kind: workflow, use: inner }}\n{after}");
    let checkout = Checkout::under(root)
        .config(&format!(
            "{MOCK_CONFIG}limits:\n  max_tokens_per_run: 100\n"
        ))
        .file(".yunta/workflows/inner.yaml", OVER_BUDGET)
        .file("fixture.yaml", OVER_BUDGET_FIXTURE)
        .workflow("wf", &parent)
        .committed();
    let run = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    );
    let id = full_run_id(&checkout.home, &run_id_from(&run));
    (checkout.repo, checkout.home, id)
}

/// The child the parent `id` is parked on, by the command the parent offers for it.
fn child_offered_by(repo: &Path, home: &Path, id: &str) -> String {
    let status = yunta_in!(repo, home, &["status", id, "--json"]);
    let document: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    let next = document["next"].as_array().expect("what comes next");
    let step = next
        .iter()
        .find(|step| step["why"] == "shows what it is waiting on")
        .unwrap_or_else(|| panic!("nothing reaches the child: {document:#}"));
    let command = step["command"].as_str().expect("its command");
    command
        .strip_prefix("yunta status ")
        .unwrap_or_else(|| panic!("{command}"))
        .to_string()
}

/// What stopped a child stands on the child's log, so the parent's page and its document
/// each hand their reader the way there, and what it shows is the child's decision.
///
/// Reached by: `crates/replay/src/frame/actions.rs`
#[test]
fn the_parent_of_a_stopped_child_offers_the_command_that_reaches_it() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, id) = parent_of_a_child_on_the_cap(root.path(), "");
    let child = child_offered_by(&repo, &home, &id);
    assert_ne!(child, id);

    let page = yunta_in!(&repo, &home, &["status", &id]);
    let said = unwrapped(&stdout(&page));
    let words = format!(
        "yunta status {} shows what it is waiting on",
        handle(&child)
    );
    assert!(said.contains(&words), "no `{words}` in: {said}");

    let reached = yunta_in!(&repo, &home, &["status", &child]);
    let said = unwrapped(&stdout(&reached));
    for words in ["decision on the run", "continue: Continue past the cap"] {
        assert!(said.contains(words), "no `{words}` in: {said}");
    }
}

/// The child is answered where it stopped and ends in an engine of its own, and the parent
/// takes it from there when it is handed back.
#[test]
fn a_child_answered_on_its_own_log_ends_and_its_parent_takes_it_from_there() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, id) = parent_of_a_child_on_the_cap(root.path(), "");
    let child = child_offered_by(&repo, &home, &id);

    let refused = yunta_in!(&repo, &home, &["resolve-gate", &id, "continue"]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &child, "continue"]);
    assert!(answered.status.success(), "{}", stderr(&answered));
    waits_for_event(&home, &child, "run_finished", 1);

    let resumed = yunta_in!(&repo, &home, &["resume", &id]);
    assert!(resumed.status.success(), "{}", stderr(&resumed));
    let log = log_of(&home, &id);
    assert_eq!(count(&log, "run_finished"), 1, "{log}");
    assert_eq!(
        count(&log, "gate_resolved"),
        0,
        "answered on the child: {log}"
    );
}

/// A `continue` on the child is for the child's cap. The parent's stands, so the parent
/// stops on it once the child's spend reaches it, and asks for itself.
#[test]
fn a_continue_on_the_child_leaves_the_parent_under_its_own_cap() {
    let root = tempfile::tempdir().unwrap();
    let last =
        "  - { id: last, kind: prompt, runner: executor, depends_on: [compose], prompt: \"Last.\" }\n";
    let (repo, home, id) = parent_of_a_child_on_the_cap(root.path(), last);
    let child = child_offered_by(&repo, &home, &id);

    let answered = yunta_in!(&repo, &home, &["resolve-gate", &child, "continue"]);
    assert!(answered.status.success(), "{}", stderr(&answered));
    waits_for_event(&home, &child, "run_finished", 1);

    let resumed = yunta_in!(&repo, &home, &["resume", &id]);
    assert_eq!(resumed.status.code(), Some(3), "{}", stderr(&resumed));
    let status = yunta_in!(&repo, &home, &["status", &id]);
    let said = unwrapped(&stdout(&status));
    assert!(said.contains("decision on the run"), "{said}");
    assert!(said.contains("continue: Continue past the cap"), "{said}");
}
