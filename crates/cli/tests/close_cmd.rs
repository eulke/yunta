//! `yunta close`: a stopped run nobody is going to continue is closed as
//! cancelled, by who closed it, and stops waiting on a person; a run an
//! engine is driving is not, and a run whose engine died is settled first.

use serde_json::Value;
use yunta_testkit::{run_id_from, runs_root, stderr, stdout, wait_until, yunta_at, Checkout};

/// A build that fails at once: the run stops on a person.
const FAILS: &str = "name: fails\nnodes:\n  - { id: build, kind: bash, run: \"exit 101\" }\n";

/// A node that says it started and holds until told to go.
fn holding(started: &std::path::Path, go: &std::path::Path) -> String {
    format!(
        "name: hold\nnodes:\n  - id: hold\n    kind: bash\n    run: \"echo in > {}; until [ -f {} ]; do sleep 0.05; done\"\n",
        started.display(),
        go.display()
    )
}

fn events(checkout: &Checkout, run_id: &str) -> Vec<Value> {
    std::fs::read_to_string(runs_root(&checkout.home).join(run_id).join("events.jsonl"))
        .expect("the exported event log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("stored event"))
        .collect()
}

fn parked() -> (Checkout, String) {
    let checkout = Checkout::new()
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", FAILS)
        .committed();
    let run = yunta_at!(&checkout, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    let run_id = run_id_from(&run);
    (checkout, run_id)
}

#[test]
fn closing_a_parked_run_records_it_cancelled_by_who_closed_it() {
    let (checkout, run_id) = parked();

    let close = yunta_at!(&checkout, &["close", &run_id, "--by", "lead"]);
    assert!(close.status.success(), "{}", stderr(&close));
    assert!(
        stdout(&close).contains("closed as cancelled by lead"),
        "{}",
        stdout(&close)
    );

    let events = events(&checkout, &run_id);
    let last = events.last().expect("events");
    assert_eq!(last["kind"], "run_finished", "{last}");
    assert_eq!(last["terminal_state"], "cancelled", "{last}");
    assert_eq!(last["closed_by"], "lead", "{last}");

    let status = yunta_at!(&checkout, &["status", &run_id, "--json"]);
    let document: Value = serde_json::from_str(&stdout(&status)).expect("one document");
    assert_eq!(document["outcome"], "cancelled");
}

#[test]
fn a_closed_run_leaves_the_inbox_and_is_closed_once() {
    let (checkout, run_id) = parked();
    let before = stdout(&yunta_at!(&checkout, &["list", "--runs"]));
    assert!(before.starts_with("needs you (1)"), "{before}");

    let close = yunta_at!(&checkout, &["close", &run_id]);
    assert!(close.status.success(), "{}", stderr(&close));

    let after = stdout(&yunta_at!(&checkout, &["list", "--runs"]));
    assert!(
        !after.contains("needs you") && after.contains("closed (1)"),
        "{after}"
    );
    let again = yunta_at!(&checkout, &["close", &run_id]);
    assert!(!again.status.success(), "a closed run is closed once");
    assert!(
        stderr(&again).contains("already closed"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn a_live_run_is_not_closed_and_cancel_is_named() {
    let markers = tempfile::tempdir().unwrap();
    let (started, go) = (markers.path().join("started"), markers.path().join("go"));
    let checkout = Checkout::new()
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", &holding(&started, &go))
        .committed();
    let detached = yunta_at!(&checkout, &["run", "wf.yaml", "--detach"]);
    let run_id = run_id_from(&detached);
    wait_until(
        || started.exists(),
        || "the detached engine never reached the node".into(),
    );

    let close = yunta_at!(&checkout, &["close", &run_id]);
    std::fs::write(&go, "go").unwrap();
    assert!(!close.status.success());
    let said = stderr(&close);
    assert!(
        said.contains("an engine is driving the run")
            && said.contains(&format!("yunta cancel {run_id}")),
        "{said}"
    );
}

#[test]
fn close_of_a_stalled_run_records_the_crash_then_closes() {
    let markers = tempfile::tempdir().unwrap();
    let (started, go) = (markers.path().join("started"), markers.path().join("go"));
    let checkout = Checkout::new()
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", &holding(&started, &go))
        .committed();
    let detached = yunta_at!(&checkout, &["run", "wf.yaml", "--detach"]);
    let run_id = run_id_from(&detached);
    wait_until(
        || started.exists(),
        || "the detached engine never reached the node".into(),
    );
    let registry = runs_root(&checkout.home)
        .join(&run_id)
        .join("scratch")
        .join("engine.json");
    let engine: Value = serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
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

    let close = yunta_at!(&checkout, &["close", &run_id]);
    std::fs::write(&go, "go").unwrap();
    assert!(close.status.success(), "{}", stderr(&close));
    let kinds: Vec<String> = events(&checkout, &run_id)
        .iter()
        .rev()
        .take(2)
        .map(|event| event["kind"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        kinds,
        ["run_finished", "run_paused"],
        "the crash, then the close"
    );
}
