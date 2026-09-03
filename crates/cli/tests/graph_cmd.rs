//! `yunta graph`: a pure derivation of the DAG from the workflow
//! definition — Mermaid by default, DOT on request — with `depends_on`
//! edges, `on_failure.goto` edges visually differentiated from them,
//! and, given a `--run <id>`, each node's derived state (via
//! `yunta_engine::derive`) annotated: no new events, no agent involved
//! in producing the graph itself, same as `status`.

use yunta_testkit::{init_repo, stdout, write, yunta_in};

const WORKFLOW: &str = r#"
name: graph-fixture
nodes:
  - id: lint
    kind: bash
    run: "true"
    on_failure: { goto: fix-lint, max_reroutes: 2 }
  - id: fix-lint
    kind: bash
    run: "true"
  - id: tests
    kind: bash
    depends_on: [lint]
    run: "true"
"#;

#[test]
fn graph_renders_mermaid_with_depends_on_and_differentiated_reroute_edges() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(&repo.join("wf.yaml"), WORKFLOW);

    let output = yunta_in!(&repo, &home, &["graph", "wf.yaml"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);

    assert!(text.trim_start().starts_with("graph TD"), "got: {text}");
    for id in ["lint", "fix-lint", "tests"] {
        assert!(text.contains(id), "missing node `{id}` in: {text}");
    }
    // depends_on: tests depends on lint -> lint runs before tests.
    assert!(
        text.contains("lint --> tests") || text.contains("lint-->tests"),
        "missing depends_on edge lint->tests in: {text}"
    );
    // on_failure.goto must be visually different from a plain depends_on
    // edge (dashed `-.->`, a differentiated re-route edge) —
    // and never rendered as a plain `-->`.
    assert!(
        text.contains("lint -.-> fix-lint") || text.contains("lint-.->fix-lint"),
        "missing differentiated re-route edge lint->fix-lint in: {text}"
    );
    assert!(
        !text.contains("lint --> fix-lint"),
        "re-route edge must not look like a plain dependency edge: {text}"
    );
}

#[test]
fn graph_with_a_run_id_annotates_each_node_with_its_derived_state() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: graph-run-fixture
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(run.status.success(), "run failed: {}", stdout(&run));
    let run_id = stdout(&run)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .expect("run id in output");

    let output = yunta_in!(&repo, &home, &["graph", "wf.yaml", "--run", &run_id]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);
    assert!(
        text.to_lowercase().contains("finished"),
        "expected the finished node's derived state in: {text}"
    );
}

#[test]
fn graph_refuses_a_workflow_that_fails_check() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // depends_on references a node that doesn't exist — check must catch
    // this before graph tries to render anything.
    write(
        &repo.join("wf.yaml"),
        r#"
name: broken
nodes:
  - id: only
    kind: bash
    run: "true"
    depends_on: [ghost]
"#,
    );

    let output = yunta_in!(&repo, &home, &["graph", "wf.yaml"]);
    assert!(!output.status.success());
}

#[test]
fn graph_resolves_a_bare_catalog_name() {
    // A reference with no extension resolves through the repo catalog —
    // the same rule `check` and `run` follow — so `graph <name>` works on
    // a `.yunta/workflows/<name>.yaml` without spelling out the path.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(&repo.join(".yunta/workflows/review.yaml"), WORKFLOW);

    let output = yunta_in!(&repo, &home, &["graph", "review"]);
    assert!(
        output.status.success(),
        "a bare catalog name must resolve like check/run do — stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout(&output).trim_start().starts_with("graph TD"),
        "got: {}",
        stdout(&output)
    );
}

#[test]
fn labels_are_escaped() {
    // A node's derived-state label carries the run's own outcome text,
    // which can hold characters that break a diagram: quotes, `<`/`>`/`&`
    // (Mermaid renders labels as HTML) and backslashes (DOT). A failed
    // bash node's outcome is `exit <code>: <stderr tail>`, so its stderr
    // is a direct, controllable source of those characters.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join("wf.yaml"),
        "name: escaping\nnodes:\n  - id: boom\n    kind: bash\n    \
         run: \"printf '%s' 'a\\\"b<c>d&e' 1>&2; exit 1\"\n",
    );

    // The run fails (the node exits non-zero); its events still record the
    // failure, which is all `graph --run` derives from.
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let run_id = stdout(&run)
        .lines()
        .find_map(|line| {
            line.strip_prefix("run ")
                .and_then(|rest| rest.split(':').next())
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("run id in output: {}", stdout(&run)));

    // The raw payload must never survive verbatim into either diagram — if
    // it did, its `"`/`<`/`>` would break the syntax.
    let raw = "a\"b<c>d&e";

    let mermaid = yunta_in!(&repo, &home, &["graph", "wf.yaml", "--run", &run_id]);
    assert!(
        mermaid.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&mermaid.stderr)
    );
    let mermaid = stdout(&mermaid);
    assert!(
        !mermaid.contains(raw),
        "the raw payload leaked unescaped into the Mermaid label: {mermaid}"
    );
    for entity in ["&quot;", "&lt;", "&gt;", "&amp;"] {
        assert!(
            mermaid.contains(entity),
            "Mermaid label missing `{entity}`: {mermaid}"
        );
    }

    let dot = yunta_in!(
        &repo,
        &home,
        &["graph", "wf.yaml", "--run", &run_id, "--format", "dot"]
    );
    assert!(
        dot.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&dot.stderr)
    );
    let dot = stdout(&dot);
    assert!(
        !dot.contains(raw),
        "the raw payload leaked unescaped into the DOT label: {dot}"
    );
    assert!(
        dot.contains("a\\\"b"),
        "DOT label must escape the double quote as \\\": {dot}"
    );
}

#[test]
fn graph_renders_dot_with_solid_dependencies_and_dashed_reroutes() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(&repo.join("wf.yaml"), WORKFLOW);

    let output = yunta_in!(&repo, &home, &["graph", "wf.yaml", "--format", "dot"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&output),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = stdout(&output);
    assert!(text.trim_start().starts_with("digraph"), "got: {text}");
    assert!(
        text.contains("\"lint\" -> \"tests\";"),
        "missing dependency edge: {text}"
    );
    assert!(
        text.contains("\"lint\" -> \"fix-lint\" [style=dashed];"),
        "missing dashed re-route edge: {text}"
    );
    assert!(text.trim_end().ends_with('}'), "got: {text}");
}
