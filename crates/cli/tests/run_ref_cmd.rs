//! A command that takes a run finds it by its handle, by any part that
//! starts or ends its id, by the whole id, and by two words: `last`,
//! this repository's newest run, and `needs`, the one waiting on a
//! person. Something two runs answer to is refused naming both.

use serde_json::Value;
use yunta_testkit::{handle, run_id_from, stderr, stdout, yunta_at, Checkout};

const FINISHES: &str = "name: ok\nnodes:\n  - { id: only, kind: bash, run: \"true\" }\n";
/// A build that fails at once: the run stops on a person.
const FAILS: &str = "name: fails\nnodes:\n  - { id: build, kind: bash, run: \"exit 101\" }\n";

fn checkout() -> Checkout {
    Checkout::new()
        .config("defaults:\n  isolation: none\n")
        .workflow("ok", FINISHES)
        .workflow("fails", FAILS)
        .committed()
}

/// The whole id of a run of `workflow`, as `yunta run` names its
/// directory.
fn ran(checkout: &Checkout, workflow: &str) -> String {
    run_id_from(&yunta_at!(checkout, &["run", workflow]))
}

/// The run `reference` finds, as `status --json` names it, or what the
/// command said instead.
fn found(checkout: &Checkout, reference: &str) -> Result<String, String> {
    let status = yunta_at!(checkout, &["status", reference, "--json"]);
    if !status.status.success() {
        return Err(stderr(&status));
    }
    let document: Value = serde_json::from_str(&stdout(&status)).expect("one JSON document");
    Ok(document["run_id"].as_str().unwrap_or_default().to_string())
}

#[test]
fn a_run_is_found_by_its_handle_a_prefix_and_its_full_id() {
    let checkout = checkout();
    let run_id = ran(&checkout, "ok.yaml");
    let lower = handle(&run_id).to_lowercase();
    for reference in [&run_id, handle(&run_id), &run_id[..20], lower.as_str()] {
        assert_eq!(
            found(&checkout, reference),
            Ok(run_id.clone()),
            "{reference}"
        );
    }
}

#[test]
fn an_ambiguous_prefix_lists_every_run_it_matches() {
    let checkout = checkout();
    let first = ran(&checkout, "ok.yaml");
    let second = ran(&checkout, "ok.yaml");
    // Two runs made minutes apart share the head of their ids.
    let shared: String = first
        .chars()
        .zip(second.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect();
    assert!(!shared.is_empty(), "{first} / {second}");

    let said = found(&checkout, &shared).expect_err("a part two runs share names neither");
    assert!(
        said.contains("name one") && said.contains(&first) && said.contains(&second),
        "{said}"
    );
}

#[test]
fn last_is_the_newest_run_of_this_project() {
    let checkout = checkout();
    ran(&checkout, "ok.yaml");
    let newest = ran(&checkout, "ok.yaml");
    assert_eq!(found(&checkout, "last"), Ok(newest));
}

#[test]
fn needs_names_the_one_run_waiting_on_a_person_or_lists_them() {
    let checkout = checkout();
    ran(&checkout, "ok.yaml");
    let first = ran(&checkout, "fails.yaml");
    assert_eq!(found(&checkout, "needs"), Ok(first.clone()));

    let second = ran(&checkout, "fails.yaml");
    let said = found(&checkout, "needs").expect_err("two runs wait on a person");
    assert!(
        said.contains("2 runs of this repository need you")
            && said.contains(&first)
            && said.contains(&second),
        "{said}"
    );
}
