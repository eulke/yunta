//! `yunta receipt <run_id>` end to end: the real compiled
//! binary, same style `stats_cmd.rs`/`run_flow.rs` use — this is what
//! proves the CLI wiring (writing `receipt.md`/`receipt.json` to
//! `run.dir`, the `--json` flag, refusing a non-terminal run) on top of
//! `yunta_replay::receipt`'s own unit-tested derivation.

use yunta_testkit::{handle, init_repo, run_id_from, stderr, stdout, unwrapped, write, yunta_in};

fn bash_only_workflow() -> &'static str {
    r#"
name: bash-only-receipt
nodes:
  - id: touch
    kind: bash
    run: "echo made > made.txt"
  - id: verify
    kind: bash
    depends_on: [touch]
    run: "test -f made.txt"
"#
}

#[test]
fn receipt_writes_both_formats_to_run_dir_and_prints_markdown_by_default() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/bash-only-receipt.yaml"),
        bash_only_workflow(),
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"]
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id]);
    assert!(receipt_out.status.success(), "{}", stderr(&receipt_out));
    let drawn = stdout(&receipt_out);
    let handle = yunta_testkit::handle(&run_id);
    let first = drawn.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("✓ receipt of bash-only-receipt (default) · finished")
            && first.trim_end().ends_with(&handle),
        "{drawn}"
    );
    // A bash-only workflow has no tasks at all, and nothing to prove is
    // never crossed out: it is said, with the neutral mark.
    assert!(
        yunta_testkit::checked(&drawn, "criteria")
            .unwrap_or_default()
            .contains("none declared, nothing to prove"),
        "{drawn}"
    );
    assert!(
        yunta_testkit::checked(&drawn, "baseline")
            .unwrap_or_default()
            .contains("not used by this workflow"),
        "{drawn}"
    );
    assert!(!drawn.contains('✗'), "{drawn}");

    // The file holds the same document, written as Markdown.
    let run_dir = home.join("runs").join(&run_id);
    let written_md = std::fs::read_to_string(run_dir.join("receipt.md")).unwrap();
    assert!(
        written_md.starts_with(&format!(
            "# ✓ receipt of bash-only-receipt (default) · finished · {handle}"
        )),
        "{written_md}"
    );
    assert!(
        written_md
            .lines()
            .any(|l| l == "- ◌ **criteria** none declared, nothing to prove"),
        "{written_md}"
    );
    assert!(
        written_md.lines().any(|l| l == format!("- run: {run_id}")),
        "{written_md}"
    );
    let written_json = std::fs::read_to_string(run_dir.join("receipt.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&written_json).unwrap();
    assert_eq!(parsed["run_id"], run_id);
    assert_eq!(parsed["terminal_state"], "done");
}

/// The `receipt.md` beside a run is a Markdown file whatever terminal asked
/// for it: a later node hands it to a forge as the pull request's body, which
/// draws the receipt with the Unicode set, so the file says what the run
/// removed with that set's minus too, not the hyphen an ASCII terminal reads.
#[test]
fn the_receipt_file_is_the_same_whatever_terminal_asks_for_it() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    write(&repo.join("kept.txt"), "one\ntwo\n");
    yunta_testkit::git(&repo, &["add", "kept.txt"]);
    yunta_testkit::git(&repo, &["commit", "-q", "-m", "a file to cut"]);
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/workflows/cut.yaml"),
        "name: cut\nnodes:\n  - id: cut\n    kind: bash\n    run: \"printf 'one\\\\n' > kept.txt\"\n",
    );
    let run_out = yunta_in!(&repo, &home, &["run", ".yunta/workflows/cut.yaml"]);
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);
    let written = |glyphs: &str| {
        let vars = [("YUNTA_GLYPHS".to_string(), glyphs.to_string())];
        let out = yunta_in!(&repo, &home, &["receipt", &run_id], &vars);
        assert!(out.status.success(), "{}", stderr(&out));
        let run_dir = home.join("runs").join(&run_id);
        std::fs::read_to_string(run_dir.join("receipt.md")).unwrap()
    };

    let unicode = written("unicode");
    assert!(
        unicode
            .lines()
            .any(|line| line == "- changed: 1 file, \u{2212}1"),
        "the file says the line the run removed with the Unicode minus:\n{unicode}"
    );
    assert_eq!(written("ascii"), unicode);
}

#[test]
fn receipt_json_flag_prints_the_json_that_was_written() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/bash-only-receipt.yaml"),
        bash_only_workflow(),
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"]
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id, "--json"]);
    assert!(receipt_out.status.success(), "{}", stderr(&receipt_out));
    let printed: serde_json::Value = serde_json::from_str(&stdout(&receipt_out)).unwrap();

    let run_dir = home.join("runs").join(&run_id);
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join("receipt.json")).unwrap())
            .unwrap();
    assert_eq!(printed, written);
}

#[test]
fn receipt_refuses_a_run_that_has_not_finished() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // No `on_failure` at all — a failing node just pauses the run.
    write(
        &repo.join(".yunta/workflows/never-finishes.yaml"),
        "name: never-finishes\nnodes:\n  - id: broken\n    kind: bash\n    run: \"false\"\n",
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/never-finishes.yaml"]
    );
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id]);
    assert!(!receipt_out.status.success());
    assert_eq!(
        unwrapped(&stderr(&receipt_out)),
        format!(
            "error: run `{run_id}` hasn't reached a terminal state yet: \
             a receipt is only generated once a run finishes; \
             `yunta status {}` shows where it is",
            handle(&run_id)
        ),
        "the refusal explains why a receipt cannot be built yet and says what shows the run's state"
    );

    let run_dir = home.join("runs").join(&run_id);
    assert!(!run_dir.join("receipt.md").exists());
    assert!(!run_dir.join("receipt.json").exists());
}

#[test]
fn the_receipt_carries_the_document_version() {
    // Every machine-readable document this CLI writes says which schema
    // it was written against — the receipt included, so a reader can
    // refuse one from a schema it predates instead of guessing at a
    // field it does not recognise.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/bash-only-receipt.yaml"),
        bash_only_workflow(),
    );

    let run_out = yunta_in!(
        &repo,
        &home,
        &["run", ".yunta/workflows/bash-only-receipt.yaml"]
    );
    assert!(run_out.status.success(), "{}", stderr(&run_out));
    let run_id = run_id_from(&run_out);

    let receipt_out = yunta_in!(&repo, &home, &["receipt", &run_id, "--json"]);
    assert!(receipt_out.status.success(), "{}", stderr(&receipt_out));
    let receipt: serde_json::Value = serde_json::from_str(&stdout(&receipt_out)).unwrap();
    assert!(
        receipt["schema_version"].as_u64().is_some(),
        "the receipt names its schema: {receipt:#}"
    );
}
