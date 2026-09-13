//! `yunta status` on a node that failed to close its declared documents.
//!
//! A node can declare more than one document, and each one fails on its
//! own terms. The log records every problem together with the document
//! it came from, so both surfaces say which file a problem belongs to:
//! the text a person reads lays out one block per document, and the JSON
//! a program reads carries the attribution as data.

use std::path::Path;

use yunta_testkit::{git, init_repo, run_id_from, stdout, write, yunta_in};

/// One node declaring two interpreted documents and writing both with
/// every entry left blank — two documents, each breaking several of its
/// own rules, from one attempt.
const TWO_DOCUMENTS: &str = r#"
name: two-documents
nodes:
  - id: draft
    kind: bash
    run: |
      printf 'tasks:\n  - id: T001\n    title: ""\n    scope: []\n    criteria: []\n' > {{node.artifacts}}/tasks.yaml
      printf 'findings:\n  - id: F1\n    severity: minor\n    title: ""\n    location: ""\n    detail: ""\n' > {{node.artifacts}}/findings.yaml
    artifacts:
      produces: [tasks, findings]
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

/// How a diagnostic names one of `draft`'s declared documents: the
/// directory that node writes in, which is where the close reads them.
fn staged(name: &str) -> String {
    format!("scratch/staging/draft/{name}")
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

    let (tasks_heading, tasks_problems) = document_block(&text, &staged("tasks.yaml"));
    let (findings_heading, findings_problems) = document_block(&text, &staged("findings.yaml"));

    // The §4 block counts what it lists, and says `errors` for more than
    // one — never the `error(s)` hedge.
    for heading in [&tasks_heading, &findings_heading] {
        assert!(
            !heading.contains("error(s)"),
            "the block pluralises properly: {heading}"
        );
    }
    assert_eq!(
        tasks_heading,
        format!("{}: {} errors", staged("tasks.yaml"), tasks_problems.len()),
        "the heading counts the problems under it: {text}"
    );
    assert_eq!(
        findings_heading,
        format!(
            "{}: {} errors",
            staged("findings.yaml"),
            findings_problems.len()
        ),
        "the heading counts the problems under it: {text}"
    );

    // Attribution: the tasks document's problems are about its task, the
    // findings artifact's are about its finding, and neither block
    // carries the other's.
    assert!(
        tasks_problems.iter().all(|p| p.contains("`T001`")),
        "the tasks document's block carries only the tasks document's problems: {tasks_problems:?}"
    );
    assert!(
        findings_problems.iter().all(|p| p.contains("`F1`")),
        "the findings block carries only its own problems: {findings_problems:?}"
    );
    assert!(
        tasks_problems.iter().any(|p| p.contains("title")),
        "a task with an empty `title` says so: {tasks_problems:?}"
    );
    assert!(
        findings_problems.iter().any(|p| p.contains("location")),
        "a finding with an empty `location` says so: {findings_problems:?}"
    );

    // The node list above stays one line per node, and that one line
    // still names both documents.
    let node_line = text
        .lines()
        .find(|line| line.trim_start().starts_with("draft: failed — "))
        .unwrap_or_else(|| panic!("no one-line verdict for `draft` in:\n{text}"));
    assert!(
        node_line.contains(&staged("tasks.yaml")) && node_line.contains(&staged("findings.yaml")),
        "the collapsed line still names every document: {node_line}"
    );
}

#[test]
fn every_level_of_a_failure_block_hangs_one_step_under_the_line_above_it() {
    // Three levels deep: the heading, the node that failed, and each
    // document that node named. The steps come from one value, so a
    // reader follows the nesting by eye instead of measuring it — and a
    // level that started spelling its own margin would show up here as a
    // ladder with an uneven rung.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("state");
    let run_id = run_two_documents(&repo, &home);

    let text = stdout(&yunta_in!(&repo, &home, &["status", &run_id]));
    let mut lines = text.lines().skip_while(|line| *line != "failures:");
    let heading = lines.next().expect("a failed run says what failed");
    let node = lines.next().expect("the node that failed heads its detail");
    let named = staged("tasks.yaml");
    let document = lines
        .find(|line| line.trim_start().starts_with(&named))
        .expect("a document the node did not close");

    assert_eq!(indent_of(heading), 0, "{text}");
    let step = indent_of(node);
    assert!(step > 0, "the node hangs under the heading: {text}");
    assert_eq!(
        indent_of(document),
        step * 2,
        "one step per level, the same step every time: {text}"
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

    let tasks = documents
        .iter()
        .find(|d| d["path"] == staged("tasks.yaml"))
        .unwrap_or_else(|| panic!("the tasks document is named by its path: {state:#}"));
    let findings = documents
        .iter()
        .find(|d| d["path"] == staged("findings.yaml"))
        .unwrap_or_else(|| panic!("the findings artifact is named by its path: {state:#}"));

    // The kind a consumer needs to know which shape the file had to meet.
    assert_eq!(tasks["kind"], "tasks", "{state:#}");
    assert_eq!(findings["kind"], "findings", "{state:#}");

    // And the attribution itself: a problem is reachable only through
    // the document it came from, so no consumer has to guess.
    let tasks_problems = tasks["diagnostics"].as_array().expect("tasks problems");
    let findings_problems = findings["diagnostics"]
        .as_array()
        .expect("findings problems");
    assert!(
        !tasks_problems.is_empty() && !findings_problems.is_empty(),
        "{state:#}"
    );
    assert!(
        tasks_problems.iter().all(|d| d["id"] == "T001"),
        "every tasks problem names the tasks document's own task: {state:#}"
    );
    assert!(
        findings_problems.iter().all(|d| d["id"] == "F1"),
        "every findings problem names the findings artifact's own finding: {state:#}"
    );
    assert!(
        tasks_problems
            .iter()
            .any(|d| d["problem"] == "rule" && d["code"] == "empty-title"),
        "the tasks document's task has an empty `title`: {state:#}"
    );
    assert!(
        findings_problems
            .iter()
            .any(|d| d["problem"] == "rule" && d["code"] == "empty-location"),
        "the finding has an empty `location`: {state:#}"
    );
}

/// A `kind: workflow` node whose child run produces nothing, declaring
/// an artifact of that child — the composition's two ends disagreeing
/// about what comes back.
const UNHELD_PARENT: &str = r#"
name: unheld-parent
nodes:
  - id: compose
    kind: workflow
    use: producer
    artifacts: { produces: [report.md] }
"#;

const UNHELD_CHILD: &str = r#"
name: producer
nodes:
  - id: work
    kind: bash
    run: "true"
"#;

/// Runs [`UNHELD_PARENT`] against a committed catalog holding
/// [`UNHELD_CHILD`], and hands back the run id the follow-up `status`
/// needs. The node fails: that is the point of the fixture.
fn run_unheld(repo: &Path, home: &Path) -> String {
    init_repo(repo);
    write(&repo.join(".yunta/workflows/producer.yaml"), UNHELD_CHILD);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "catalog"]);
    write(&repo.join("wf.yaml"), UNHELD_PARENT);
    let run = yunta_in!(repo, home, &["run", "wf.yaml"]);
    assert!(
        !run.status.success(),
        "the node must fail on what its child never produced: {}",
        stdout(&run)
    );
    run_id_from(&run)
}

#[test]
fn status_attributes_an_artifact_no_run_holds_to_that_artifact() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("state");
    let run_id = run_unheld(&repo, &home);

    let status = yunta_in!(&repo, &home, &["status", &run_id]);
    let text = stdout(&status);
    assert!(
        text.lines().any(|line| line.trim() == "compose:"),
        "the failing node heads its own detail: {text}"
    );
    assert!(
        text.contains("holds no artifact `report.md`"),
        "the block says which artifact did not close: {text}"
    );
}

#[test]
fn status_json_publishes_an_unheld_artifact_under_its_stable_code() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("state");
    let run_id = run_unheld(&repo, &home);

    let status = yunta_in!(&repo, &home, &["status", &run_id, "--json"]);
    let state: serde_json::Value = serde_json::from_slice(&status.stdout)
        .unwrap_or_else(|e| panic!("status --json emits JSON: {e}\n{}", stdout(&status)));

    let entries = state["diagnostics"]["compose"]
        .as_array()
        .unwrap_or_else(|| panic!("the failed node's artifacts are data: {state:#}"));
    assert_eq!(entries.len(), 1, "one artifact did not close: {state:#}");
    let entry = &entries[0];
    assert_eq!(entry["code"], "artifact-unheld", "{state:#}");
    assert_eq!(entry["artifact"]["name"], "report.md", "{state:#}");
    // The run a reader has to go look at is the child's, not this one's.
    let child = entry["run"].as_str().expect("the run it was missing from");
    assert_ne!(child, run_id, "{state:#}");
    assert!(!child.is_empty(), "{state:#}");
    // Nothing read a document, so the entry claims neither a path nor a
    // kind nor a problem inside a file.
    assert!(entry["path"].is_null(), "{state:#}");
    assert!(entry["kind"].is_null(), "{state:#}");
    assert!(entry["file"].is_null(), "{state:#}");
}
