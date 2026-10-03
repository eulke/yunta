//! Every surface answers the three questions a person brings to a run:
//! does it need me, why did it fail, and what do I type next. Each test
//! asks one of them of every surface that shows the run.

use std::path::{Path, PathBuf};

use serde_json::Value;
use yunta_testkit::{handle, run_id_from, stderr, stdout, yunta_in, yunta_on_terminal, Checkout};

/// A node that fails with its re-routes already spent: the run parks on
/// a decision whose menu its log rebuilds.
const EXHAUSTED: &str = r#"
name: hopeless
nodes:
  - id: lint
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix-lint, max_reroutes: 0 }
  - id: fix-lint
    kind: bash
    run: "touch fixed.txt"
"#;

/// A build that fails the way a compiler does: what went wrong is on
/// stdout, and the exit code says only that it did.
const COMPILER: &str = r#"
name: compiler
nodes:
  - id: build
    kind: bash
    run: |
      printf 'error[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:3:5\n'
      exit 101
"#;

/// What the compiler printed, which every surface owes the reader.
const PRINTED: &str = "error[E0425]: cannot find value `x` in this scope";

fn project(root: &Path, config: &str, workflow: &str) -> (PathBuf, PathBuf) {
    let checkout = Checkout::under(root)
        .config(config)
        .workflow("wf", workflow)
        .committed();
    (checkout.repo, checkout.home)
}

/// Runs `command` — a line a surface printed — as typed.
fn typed(repo: &Path, home: &Path, command: &str) -> std::process::Output {
    let words: Vec<&str> = command.split_whitespace().skip(1).collect();
    yunta_in!(repo, home, &words)
}

/// The first line of `text` that, trimmed, opens with `command`.
fn line_starting<'a>(text: &'a str, command: &str) -> &'a str {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with(command))
        .unwrap_or_else(|| panic!("no `{command}` line in:\n{text}"))
}

#[test]
fn a_person_learns_a_run_needs_them_from_status_and_the_list() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), "defaults:\n  isolation: none\n", EXHAUSTED);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert_eq!(run.status.code(), Some(3), "{}", stderr(&run));
    let run_id = run_id_from(&run);

    // The page says it in its first two lines: the word, and what holds
    // the run.
    let status = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    let mut lines = status.lines();
    assert!(
        lines.next().is_some_and(|line| line.ends_with("needs you")),
        "{status}"
    );
    assert!(
        lines
            .next()
            .is_some_and(|line| line.contains("re-routes to `fix-lint` are exhausted")),
        "{status}"
    );

    // The list opens with the runs that need someone, this one among
    // them, saying why.
    let list = stdout(&yunta_in!(&repo, &home, &["list", "--runs"]));
    assert!(list.starts_with("needs you (1)"), "{list}");
    assert!(
        list.contains(handle(&run_id)) && list.contains("re-routes to `fix-lint` are exhausted"),
        "{list}"
    );
}

#[test]
fn a_person_learns_why_it_failed_on_every_surface() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(
        root.path(),
        "defaults:\n  isolation: none\n  on_failure: abort\n",
        COMPILER,
    );
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let run_id = run_id_from(&run);

    for (surface, text) in [
        ("the lines a pipe gets", stderr(&run)),
        ("the block that closes the run", stdout(&run)),
        (
            "the status page",
            stdout(&yunta_in!(&repo, &home, &["status", &run_id])),
        ),
        (
            "the node's own page",
            stdout(&yunta_in!(
                &repo,
                &home,
                &["status", &run_id, "--node", "build"]
            )),
        ),
        (
            "the receipt",
            stdout(&yunta_in!(&repo, &home, &["receipt", &run_id])),
        ),
    ] {
        assert!(
            text.contains(PRINTED),
            "{surface} never quotes what the compiler printed:\n{text}"
        );
    }
}

#[test]
fn a_person_can_type_the_next_command_on_every_surface() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home) = project(root.path(), "defaults:\n  isolation: none\n", EXHAUSTED);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let run_id = run_id_from(&run);
    let called = handle(&run_id);

    // The page names the command for each option, and the list the page
    // to read: each runs as printed.
    let status = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    line_starting(&status, &format!("yunta resolve-gate {called} retry"));
    let list = stdout(&yunta_in!(&repo, &home, &["list", "--runs"]));
    let read = typed(&repo, &home, line_starting(&list, "yunta status"));
    assert!(read.status.success(), "{}", stderr(&read));

    // The block the run closed with names the command that answers it,
    // and typed as printed it does.
    let closing = stdout(&run);
    let answer = line_starting(&closing, &format!("yunta resolve-gate {called} abort"));
    let answered = typed(&repo, &home, answer);
    assert!(answered.status.success(), "{}", stderr(&answered));
    assert!(
        stdout(&answered).contains("resolved `abort`"),
        "{}",
        stdout(&answered)
    );
}

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
fn drawn_run_id(drawn: &str) -> String {
    drawn
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| word.len() == 26 && word.starts_with("01"))
        .expect("the run drew its id")
        .to_string()
}

#[test]
fn a_person_learns_a_run_asking_at_its_terminal_needs_them() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config("defaults:\n  isolation: none\n")
        .workflow("wf", ASKS)
        .committed();
    let (repo, home) = (&checkout.repo, &checkout.home);
    let mut terminal = yunta_on_terminal!(repo, home, &["run", "wf.yaml", "--quiet"]);
    terminal.wait_for("2  abort", "the gate never put its options on the console");
    let run_id = drawn_run_id(&terminal.drawn());

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
