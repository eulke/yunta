//! `yunta status` on a node that failed to close its declared documents.
//!
//! A node can declare more than one document, and each one fails on its
//! own terms. The log records every problem together with the document
//! it came from, so both surfaces say which file a problem belongs to:
//! the text a person reads lays out one block per document, and the JSON
//! a program reads carries the attribution as data.

use std::path::Path;

use yunta_testkit::{init_repo, run_id_from, stdout, write, yunta_in};

/// One node declaring two interpreted documents and writing both with
/// their required keys left out — two documents, each with problems of
/// its own, from one attempt.
const TWO_DOCUMENTS: &str = r#"
name: two-documents
nodes:
  - id: draft
    kind: bash
    run: |
      mkdir -p {{run.dir}}/artifacts
      printf 'tasks:\n  - id: T001\n' > {{run.dir}}/artifacts/plan.yaml
      printf 'findings:\n  - id: F1\n' > {{run.dir}}/artifacts/notes.yaml
    artifacts:
      produces:
        - { name: plan.yaml, kind: task-ledger }
        - { name: notes.yaml, kind: findings }
"#;

/// Runs [`TWO_DOCUMENTS`] and hands back the run id the follow-up
/// `status` needs. The run fails: that is the point of the fixture.
fn run_two_documents(repo: &Path, home: &Path) -> String {
    init_repo(repo);
    write(&repo.join("wf.yaml"), TWO_DOCUMENTS);
    let run = yunta_in!(repo, home, &["run", "wf.yaml"]);
    assert!(
        !run.status.success(),
        "the node must fail on its documents: {}",
        stdout(&run)
    );
    run_id_from(&run)
}

/// How deep a line is indented — what separates a document's heading
/// from the problems that hang under it.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The block one document contributes: its heading line and the problem
/// lines indented under it, both trimmed.
fn document_block(text: &str, path: &str) -> (String, Vec<String>) {
    let prefix = format!("{path}: ");
    let mut lines = text
        .lines()
        .skip_while(|line| !line.trim_start().starts_with(&prefix));
    let heading = lines
        .next()
        .unwrap_or_else(|| panic!("no block for `{path}` in:\n{text}"));
    let depth = indent_of(heading);
    let problems = lines
        .take_while(|line| indent_of(line) > depth)
        .map(|line| line.trim().to_string())
        .collect();
    (heading.trim().to_string(), problems)
}

#[test]
fn status_attributes_each_problem_to_the_document_it_came_from() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("state");
    let run_id = run_two_documents(&repo, &home);

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    let text = stdout(&status);
    assert!(
        text.lines().any(|line| line == "failures:"),
        "a failed run says what failed: {text}"
    );
    assert!(
        text.lines().any(|line| line.trim() == "draft:"),
        "the failing node heads its own detail: {text}"
    );

    let (ledger_heading, ledger_problems) = document_block(&text, "artifacts/plan.yaml");
    let (findings_heading, findings_problems) = document_block(&text, "artifacts/notes.yaml");

    // The §4 block counts what it lists, and says `errors` for more than
    // one — never the `error(s)` hedge.
    for heading in [&ledger_heading, &findings_heading] {
        assert!(
            !heading.contains("error(s)"),
            "the block pluralises properly: {heading}"
        );
    }
    assert_eq!(
        ledger_heading,
        format!("artifacts/plan.yaml: {} errors", ledger_problems.len()),
        "the heading counts the problems under it: {text}"
    );
    assert_eq!(
        findings_heading,
        format!("artifacts/notes.yaml: {} errors", findings_problems.len()),
        "the heading counts the problems under it: {text}"
    );

    // Attribution: the task ledger's problems are about its task, the
    // findings artifact's are about its finding, and neither block
    // carries the other's.
    assert!(
        ledger_problems.iter().all(|p| p.contains("`T001`")),
        "the ledger's block carries only the ledger's problems: {ledger_problems:?}"
    );
    assert!(
        findings_problems.iter().all(|p| p.contains("`F1`")),
        "the findings block carries only its own problems: {findings_problems:?}"
    );
    assert!(
        ledger_problems.iter().any(|p| p.contains("title")),
        "a task with no `title` says so: {ledger_problems:?}"
    );
    assert!(
        findings_problems.iter().any(|p| p.contains("severity")),
        "a finding with no `severity` says so: {findings_problems:?}"
    );

    // The node list above stays one line per node, and that one line
    // still names both documents.
    let node_line = text
        .lines()
        .find(|line| line.trim_start().starts_with("draft: failed — "))
        .unwrap_or_else(|| panic!("no one-line verdict for `draft` in:\n{text}"));
    assert!(
        node_line.contains("artifacts/plan.yaml") && node_line.contains("artifacts/notes.yaml"),
        "the collapsed line still names every document: {node_line}"
    );
}

#[test]
fn status_json_carries_the_document_each_problem_belongs_to() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("state");
    let run_id = run_two_documents(&repo, &home);

    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout)
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}\n{}", stdout(&status)));

    let documents = state["diagnostics"]["draft"]
        .as_array()
        .unwrap_or_else(|| panic!("the failed node's documents are data: {state:#}"));
    assert_eq!(
        documents.len(),
        2,
        "one entry per document that did not close: {state:#}"
    );

    let ledger = documents
        .iter()
        .find(|d| d["path"] == "artifacts/plan.yaml")
        .unwrap_or_else(|| panic!("the ledger is named by its path: {state:#}"));
    let findings = documents
        .iter()
        .find(|d| d["path"] == "artifacts/notes.yaml")
        .unwrap_or_else(|| panic!("the findings artifact is named by its path: {state:#}"));

    // The kind a consumer needs to know which shape the file had to meet.
    assert_eq!(ledger["kind"], "task-ledger", "{state:#}");
    assert_eq!(findings["kind"], "findings", "{state:#}");

    // And the attribution itself: a problem is reachable only through
    // the document it came from, so no consumer has to guess.
    let ledger_problems = ledger["diagnostics"].as_array().expect("ledger problems");
    let findings_problems = findings["diagnostics"]
        .as_array()
        .expect("findings problems");
    assert!(
        !ledger_problems.is_empty() && !findings_problems.is_empty(),
        "{state:#}"
    );
    assert!(
        ledger_problems.iter().all(|d| d["id"] == "T001"),
        "every ledger problem names the ledger's own task: {state:#}"
    );
    assert!(
        findings_problems.iter().all(|d| d["id"] == "F1"),
        "every findings problem names the findings artifact's own finding: {state:#}"
    );
    assert!(
        ledger_problems
            .iter()
            .any(|d| d["problem"] == "missing-key" && d["key"] == "title"),
        "the ledger's task has no `title`: {state:#}"
    );
    assert!(
        findings_problems
            .iter()
            .any(|d| d["problem"] == "missing-key" && d["key"] == "severity"),
        "the finding has no `severity`: {state:#}"
    );
}
