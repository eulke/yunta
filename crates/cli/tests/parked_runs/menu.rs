//! The menu of a parked run as a program reads it: its options, the command that
//! answers it, and the failure carried as data.

use super::*;

#[test]
fn the_menu_a_person_reads_is_the_menu_a_program_reads() {
    let root = tempfile::tempdir().unwrap();
    let repo = repo_with(root.path(), &[("hopeless", EXHAUSTED_REROUTE)]);
    let home = root.path().join("state");

    let run = yunta_in!(&repo, &home, &["run", "hopeless.yaml"]);
    let run_id = run_id_from(&run);
    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout)
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}\n{}", stdout(&status)));

    assert_decision_menu(&state, &run_id);

    assert_eq!(state["schema_version"], 7, "{state:#}");
    assert!(
        state["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("needs you — node `lint` failed"),
        "{state:#}"
    );
    assert!(
        state["nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["id"] == "lint")),
        "{state:#}"
    );
}

/// The decision a parked run holds, as a program reads it: the node it is
/// about, its options with their ids and tradeoffs, the command that answers
/// it, and the failure with the facts a person reads beside it.
fn assert_decision_menu(state: &serde_json::Value, run_id: &str) {
    let decision = &state["decision"];
    assert_eq!(decision["node"], "lint", "{state:#}");
    let options = decision["options"]
        .as_array()
        .unwrap_or_else(|| panic!("the menu is data, not a sentence: {state:#}"));
    let ids: Vec<&str> = options.iter().filter_map(|o| o["id"].as_str()).collect();
    assert_eq!(ids, ["retry", "abort"], "{state:#}");
    assert!(
        options
            .iter()
            .all(|option| !option["tradeoff"].as_str().unwrap_or_default().is_empty()),
        "every option carries its tradeoff: {state:#}"
    );
    assert_eq!(
        decision["resolve_with"],
        serde_json::Value::String(format!("yunta resolve-gate {run_id} <option>")),
        "{state:#}"
    );
    // A failure's menu carries the failure as data. The facts a person reads
    // beside it are written from that data: its headline, and no detail since
    // the command printed nothing.
    assert_eq!(
        decision["evidence"],
        serde_json::json!([{ "value": "exit 1" }]),
        "{state:#}"
    );
    assert_eq!(
        decision["failure"]["failure"]["exited"]["code"], 1,
        "{state:#}"
    );
}
