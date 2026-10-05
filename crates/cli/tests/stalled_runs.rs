//! A run whose engine died: its log still says it is moving, and every
//! surface that reads it from outside says nothing is driving it.

use serde_json::Value;
use yunta_testkit::{
    full_run_id, run_id_from, runs_root, stderr, stdout, wait_until, yunta_at, Checkout,
};

/// A node that says it started and then holds until it is told to go,
/// so the run is mid-node when its engine is killed — or until its
/// marker is gone with the test, so no engine outlives the test that
/// started it.
fn holding(started: &std::path::Path, go: &std::path::Path) -> String {
    format!(
        "name: hold\nnodes:\n  - id: hold\n    kind: bash\n    run: \"echo in > {0}; until [ -f {1} ] || [ ! -e {0} ]; do sleep 0.05; done\"\n",
        started.display(),
        go.display()
    )
}

/// Kills the engine driving `run_id` the way a crash would, leaving its
/// registry behind, and waits until the host says it is gone.
fn kill_the_engine(checkout: &Checkout, run_id: &str) {
    let registry = runs_root(&checkout.home)
        .join(full_run_id(&checkout.home, run_id))
        .join("scratch")
        .join("engine.json");
    let engine: Value =
        serde_json::from_slice(&std::fs::read(&registry).expect("the engine's registry"))
            .expect("a registry that reads");
    let pid = engine["engine_pid"].to_string();
    let signal = |args: &[&str]| {
        std::process::Command::new("kill")
            .args(args)
            .status()
            .expect("kill")
    };
    assert!(signal(&["-9", &pid]).success(), "the engine was killed");
    wait_until(
        || !signal(&["-0", &pid]).success(),
        || "the killed engine is still there".into(),
    );
}

#[test]
fn a_run_whose_engine_was_killed_is_listed_and_shown_as_stalled() {
    let markers = tempfile::tempdir().unwrap();
    let (started, go) = (markers.path().join("started"), markers.path().join("go"));
    let checkout = Checkout::new()
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", &holding(&started, &go))
        .committed();

    let detached = yunta_at!(&checkout, &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    let run_id = run_id_from(&detached);
    wait_until(
        || started.exists(),
        || "the detached engine never reached the node".into(),
    );

    kill_the_engine(&checkout, &run_id);
    // The node's own command holds until this exists.
    std::fs::write(&go, "go").unwrap();

    let status = yunta_at!(&checkout, &["status", &run_id]);
    let page = stdout(&status);
    assert!(status.status.success(), "{}", stderr(&status));
    let lines: Vec<&str> = page.lines().collect();
    assert!(
        lines[0].ends_with("stalled")
            && lines[1] == "  no process is driving it: the engine that ran it is gone",
        "the page says nothing drives the run:\n{page}"
    );
    assert!(
        page.contains(&format!("yunta resume {run_id}")),
        "the page names the command that hands the run back:\n{page}"
    );

    let json = yunta_at!(&checkout, &["status", &run_id, "--json"]);
    assert!(
        json.status.success(),
        "status exits 0 whatever the run says"
    );
    let document: Value = serde_json::from_str(&stdout(&json)).expect("one JSON document");
    assert_eq!(document["outcome"], "stalled", "{document:#}");

    let list = yunta_at!(&checkout, &["list", "--runs"]);
    let inbox = stdout(&list);
    assert!(
        inbox.lines().any(|line| line == "stalled (1)") && inbox.contains(&run_id),
        "the inbox files the run under stalled:\n{inbox}"
    );
}

#[test]
fn a_run_whose_engine_is_alive_is_never_called_stalled() {
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

    let json = yunta_at!(&checkout, &["status", &run_id, "--json"]);
    std::fs::write(&go, "go").unwrap();
    let document: Value = serde_json::from_str(&stdout(&json)).expect("one JSON document");
    assert_eq!(document["outcome"], "running", "{document:#}");
}
