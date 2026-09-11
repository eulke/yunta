use std::path::Path;
use std::process::Command;

fn yunta(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(args)
        .output()
        .expect("failed to run the yunta binary")
}

fn write(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn a_well_formed_workflow_checks_clean_with_no_runner_fields() {
    let dir = tempfile::tempdir().unwrap();
    let workflow = write(
        dir.path(),
        "workflow.yaml",
        r#"
name: fixture
nodes:
  - id: lint
    kind: bash
    run: "true"
"#,
    );

    let output = yunta(&["check", workflow.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK"));
}

#[test]
fn an_unresolved_runner_fails_check_and_names_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let workflow = write(
        dir.path(),
        "workflow.yaml",
        r#"
name: fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "do the thing"
"#,
    );

    let output = yunta(&["check", workflow.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|l| l
                == "  node `plan` references runner `planner`, which `runners:` does not define"),
        "check names the unresolved runner and the field that must define it: {stderr}"
    );
}

#[test]
fn a_config_file_resolves_the_runner() {
    let dir = tempfile::tempdir().unwrap();
    let workflow = write(
        dir.path(),
        "workflow.yaml",
        r#"
name: fixture
nodes:
  - id: plan
    kind: prompt
    runner: planner
    prompt: "do the thing"
"#,
    );
    let config = write(
        dir.path(),
        "config.yaml",
        r#"
runners:
  planner:
    - { adapter: mock, model: mock-model }
"#,
    );

    let output = yunta(&[
        "check",
        workflow.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_missing_workflow_file_is_a_clean_error_not_a_panic() {
    let output = yunta(&["check", "/nonexistent/workflow.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("error: failed to read workflow at /nonexistent/workflow.yaml"),
        "a missing workflow file is one clean read error, not a panic: {stderr}"
    );
}

#[test]
fn malformed_yaml_is_a_clean_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let workflow = write(dir.path(), "workflow.yaml", "not: [valid, workflow");

    let output = yunta(&["check", workflow.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("error: failed to parse workflow at "),
        "malformed YAML is one clean parse error, not a panic: {stderr}"
    );
}

#[test]
fn a_repo_re_allowing_an_org_denied_pattern_fails_check_citing_the_layer() {
    // The org layer denies `sudo *`; the repo layer tries to
    // re-allow it. `yunta check` (no --config: the project's real layers)
    // must refuse, naming both layers and the pattern.
    let root = tempfile::tempdir().unwrap();
    let org_config = write(
        root.path(),
        "org.yaml",
        "permissions:\n  commands:\n    deny: [\"sudo *\"]\n",
    );
    let home = root.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "permissions:\n  commands:\n    allow: [\"sudo *\"]\n",
    )
    .unwrap();
    let workflow = write(
        &repo,
        "workflow.yaml",
        "name: fixture\nnodes:\n  - id: lint\n    kind: bash\n    run: \"true\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["check", workflow.to_str().unwrap()])
        .current_dir(&repo)
        .env("YUNTA_ORG_CONFIG", &org_config)
        .env("YUNTA_HOME", &home)
        .output()
        .expect("failed to run the yunta binary");

    assert!(!output.status.success(), "check must refuse the re-allow");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|l| l
            == "  layer `repo` re-allows command pattern `sudo *` denied by layer `org` — permissions only narrow"),
        "the refusal cites both layers and the re-allowed pattern: {stderr}"
    );
}

#[test]
fn a_workflow_command_denied_by_the_org_layer_fails_check() {
    let root = tempfile::tempdir().unwrap();
    let org_config = write(
        root.path(),
        "org.yaml",
        "permissions:\n  commands:\n    deny: [\"sudo *\"]\n",
    );
    let home = root.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let workflow = write(
        &repo,
        "workflow.yaml",
        "name: fixture\nnodes:\n  - id: escalate\n    kind: bash\n    run: \"sudo make install\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_yunta"))
        .args(["check", workflow.to_str().unwrap()])
        .current_dir(&repo)
        .env("YUNTA_ORG_CONFIG", &org_config)
        .env("YUNTA_HOME", &home)
        .output()
        .expect("failed to run the yunta binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|l| l
            == "  node `escalate`: command `sudo make install` matches denied pattern `sudo *` (permissions.commands.deny)"),
        "the refusal cites the node, its command and the rule it matched: {stderr}"
    );
}

/// The §4 block heads a file's problems with how many there are. It
/// counts, and it says `error` or `errors` accordingly — one home for
/// the block means every surface that lists a file's problems reads the
/// same way.
#[test]
fn the_error_block_counts_what_it_lists_and_pluralises_it() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let home = root.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    // One node with a runner nothing defines: exactly one error.
    write(
        &repo,
        "one.yaml",
        "name: one\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: hi\n",
    );
    let output = yunta_testkit::yunta_in!(&repo, &home, &["check", "one.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|l| l == "one.yaml: 1 error"),
        "a single problem reads `1 error`: {stderr}"
    );

    // A second node with the same defect: two errors, plural.
    write(
        &repo,
        "two.yaml",
        "name: two\nnodes:\n  - id: plan\n    kind: prompt\n    runner: planner\n    prompt: hi\n  \
         - id: build\n    kind: prompt\n    runner: builder\n    prompt: hi\n",
    );
    let output = yunta_testkit::yunta_in!(&repo, &home, &["check", "two.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.lines().any(|l| l == "two.yaml: 2 errors"),
        "two problems read `2 errors`: {stderr}"
    );
    assert!(
        !stderr.contains("error(s)"),
        "the count is known, so the plural is not hedged: {stderr}"
    );
}
