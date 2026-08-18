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
    assert!(stderr.contains("runner `planner`"));
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
    assert!(stderr.contains("failed to read workflow"));
}

#[test]
fn malformed_yaml_is_a_clean_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let workflow = write(dir.path(), "workflow.yaml", "not: [valid, workflow");

    let output = yunta(&["check", workflow.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to parse workflow"));
}
