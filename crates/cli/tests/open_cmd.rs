//! `yunta open <run>` end to end, with the real binary. Each test's stdout is a
//! pipe, so no browser opens: a script gets told where the page is.

use std::path::{Path, PathBuf};

use yunta_testkit::{handle, init_repo, run_id_from, stderr, stdout, write, yunta_in, Checkout};

const BASH_ONLY: &str = r#"
name: bash-only
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
  - id: verify
    kind: bash
    depends_on: [touch]
    run: "test -f made.txt"
"#;

/// A planner, and the gate that shows its plan.
const REVIEWED_PLAN: &str = r#"
name: reviewed-plan
nodes:
  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    prompt: "Hand over the tasks document."
    artifacts:
      produces: [tasks]
  - id: approve-plan
    kind: gate
    assignee: lead
    message: "Plan registered. Approve?"
    options: [approve, adjust]
    on: { adjust: plan }
    shows: [{ node: plan, kind: tasks }]
"#;

/// The planner, handing over a plan that widens `build_it`.
const PLANNER: &str = r#"
dialect: claude-code
sessions:
  - steps:
      - type: run_tool
        tool: yunta_submit_tasks
        arguments:
          document:
            summary: "Widen build_it"
            description: "build_it takes a width beside its depth."
            shapes:
              - { name: build_it, owner: T001, file: src/lib.rs, code: "pub fn build_it(depth: u32, width: u32);" }
            tasks:
              - { id: T001, title: "Widen build_it", description: "Gives build_it a width.", scope: [src/lib.rs], changes: [{ at: "src/lib.rs::build_it", what: "takes a width", code: "pub fn build_it(depth: u32, width: u32);" }], outcome: "build_it takes a width", criteria: [{ cmd: "test -f built.txt", proves: "the width is built" }] }
    outcome: { type: completed, summary: "planned" }
"#;

const RUN: [&str; 7] = [
    "run",
    "wf.yaml",
    "--adapter",
    "mock",
    "--fixture",
    "fixture.yaml",
    "--quiet",
];

/// A finished run: its repository, its state and its handle.
fn finished(root: &Path) -> (PathBuf, PathBuf, String) {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.join("state");
    write(&repo.join(".yunta/workflows/bash-only.yaml"), BASH_ONLY);
    let ran = yunta_in!(&repo, &home, &["run", ".yunta/workflows/bash-only.yaml"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    let run = handle(&run_id_from(&ran)).to_string();
    (repo, home, run)
}

/// The file `yunta open` wrote, by what it says as JSON.
fn page_at(said: &str) -> PathBuf {
    let parsed: serde_json::Value = serde_json::from_str(said).unwrap();
    PathBuf::from(parsed["page"].as_str().unwrap())
}

/// Whether a cell of a table's id column holds `name`.
fn has_a_row(page: &str, name: &str) -> bool {
    page.split(r#"<td class="id">"#).skip(1).any(|cell| {
        cell.split("</td>")
            .next()
            .unwrap_or_default()
            .contains(name)
    })
}

#[test]
fn open_writes_the_page_into_the_run_folder_and_says_where_it_is() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, run) = finished(root.path());

    let opened = yunta_in!(&repo, &home, &["open", &run]);
    assert!(opened.status.success(), "{}", stderr(&opened));
    assert!(stdout(&opened).contains("page.html"), "{}", stdout(&opened));

    let said = yunta_in!(&repo, &home, &["open", &run, "--json"]);
    assert!(said.status.success(), "{}", stderr(&said));
    let at = page_at(&stdout(&said));
    assert_eq!(at.file_name().unwrap(), "page.html");
    assert!(
        at.parent().unwrap().join("manifest.yaml").exists(),
        "{}",
        at.display()
    );
    let page = std::fs::read_to_string(&at).unwrap();
    assert!(page.starts_with("<!doctype html>"), "{page}");
    assert!(page.contains(&run), "{page}");
    assert!(page.contains(r#"<main class="doc">"#), "{page}");
    assert!(has_a_row(&page, "touch"), "{page}");
    assert!(has_a_row(&page, "verify"), "{page}");
    assert!(page.contains("drawn at"), "{page}");
    assert!(!page.contains("<aside"), "it waits on nothing: {page}");
}

#[test]
fn a_decision_stands_beside_the_documents_it_is_about() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .config(
            "defaults:\n  isolation: none\nrunners:\n  planner:\n    \
             - { adapter: mock, model: mock-model }\n",
        )
        .workflow("wf", REVIEWED_PLAN)
        .file("fixture.yaml", PLANNER)
        .file("src/lib.rs", "pub fn build_it(depth: u32) {}\n")
        .file("src/other.rs", "fn caller() { build_it(1); }\n")
        .committed();
    let parked = yunta_in!(&checkout.repo, &checkout.home, &RUN);
    assert_eq!(parked.status.code(), Some(3), "{}", stderr(&parked));
    let run = handle(&run_id_from(&parked)).to_string();

    let said = yunta_in!(&checkout.repo, &checkout.home, &["open", &run, "--json"]);
    assert!(said.status.success(), "{}", stderr(&said));
    let page = std::fs::read_to_string(page_at(&stdout(&said))).unwrap();

    let (document, decision) = page
        .split_once(r#"<aside class="decide""#)
        .unwrap_or_else(|| panic!("no decision: {page}"));
    assert!(
        document.contains("Widen build_it"),
        "the plan stands whole on the page: {document}"
    );
    assert!(
        document.contains("<ins>"),
        "with what it changes marked: {document}"
    );
    assert!(
        !document.contains(r#"data-copy="yunta resolve-gate"#),
        "{document}"
    );
    assert!(
        decision.contains(&format!(r#"data-copy="yunta resolve-gate {run} approve"#)),
        "{decision}"
    );
    assert!(
        decision.contains(&format!(r#"data-copy="yunta resolve-gate {run} adjust"#)),
        "{decision}"
    );
}

#[test]
fn open_refuses_a_run_it_cannot_find() {
    let root = tempfile::tempdir().unwrap();
    let (repo, home, _) = finished(root.path());
    let opened = yunta_in!(&repo, &home, &["open", "NOSUCH"]);
    assert!(!opened.status.success(), "{}", stdout(&opened));
    assert!(stderr(&opened).contains("NOSUCH"), "{}", stderr(&opened));
}
