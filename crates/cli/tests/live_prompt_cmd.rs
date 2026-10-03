//! A run whose engine is asking at the terminal that drives it needs a
//! person, and every surface read from elsewhere says so — until the
//! question is answered.

use serde_json::Value;
use yunta_testkit::{stderr, stdout, yunta_in, yunta_on_terminal, Checkout};

/// A node that finishes at once, and a gate that asks about it.
const ASKS: &str = r#"
name: asks
nodes:
  - id: build
    kind: bash
    run: "true"
  - id: approve
    kind: gate
    depends_on: [build]
    assignee: lead
    message: "Ship it?"
"#;

/// The run id a run drew first.
fn run_id(drawn: &str) -> String {
    drawn
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| word.len() == 26 && word.starts_with("01"))
        .expect("the run drew its id")
        .to_string()
}

#[test]
fn a_run_asking_at_its_terminal_is_said_to_need_you_from_anywhere() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", ASKS)
        .committed();
    let (repo, home) = (&checkout.repo, &checkout.home);
    let mut terminal = yunta_on_terminal!(repo, home, &["run", "wf.yaml", "--quiet"]);
    terminal.wait_for("2  abort", "the gate never put its options on the console");
    let run_id = run_id(&terminal.drawn());

    let status = yunta_in!(repo, home, &["status", &run_id]);
    assert!(status.status.success(), "{}", stderr(&status));
    let page = stdout(&status);
    let lines: Vec<&str> = page.lines().collect();
    assert!(
        lines[0].ends_with("needs you")
            && lines[1]
                .contains("node `approve` is asking at the terminal that runs this run (pid"),
        "the page says the run needs a person, and where to answer:\n{page}"
    );

    let json = yunta_in!(repo, home, &["status", &run_id, "--json"]);
    let document: Value = serde_json::from_str(&stdout(&json)).expect("one JSON document");
    assert_eq!(document["waiting_on"]["on"], "prompt", "{document:#}");
    assert_eq!(document["waiting_on"]["node"], "approve", "{document:#}");

    let list = yunta_in!(repo, home, &["list", "--runs"]);
    let inbox = stdout(&list);
    let handle = &run_id[run_id.len() - 6..];
    let filed = inbox
        .lines()
        .skip_while(|line| !line.starts_with("needs you"))
        .take_while(|line| !line.is_empty())
        .any(|line| line.contains(handle));
    assert!(filed, "the inbox files the run under needs you:\n{inbox}");

    // `abort`, the second option, with nothing to add.
    terminal.keys("\x1b[B\r");
    terminal.wait_for(
        "enter records it",
        "aborting asked for words it does not need",
    );
    terminal.keys("\r");
    terminal.ended();

    let after = stdout(&yunta_in!(repo, home, &["status", &run_id]));
    assert!(
        !after.contains("is asking at the terminal"),
        "a question answered is asked no longer:\n{after}"
    );
}
