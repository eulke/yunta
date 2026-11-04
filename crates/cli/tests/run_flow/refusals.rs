//! What a run refuses before it exists, and what names it when a command cannot open it.

use super::*;

#[test]
fn a_short_declared_secret_refuses_the_run_before_its_directory_exists() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    write(
        &repo.join(".yunta/config.yaml"),
        "secrets: [YUNTA_TEST_SHORT_TOKEN]\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: short-secret\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, &repo, &home);
    let run = command
        .args(["run", "wf.yaml"])
        .env("YUNTA_TEST_SHORT_TOKEN", "abc")
        .output()
        .unwrap();
    assert!(
        !run.status.success(),
        "a 3-character secret must refuse the run"
    );
    assert!(
        stderr(&run).contains("YUNTA_TEST_SHORT_TOKEN"),
        "the refusal names the variable: {}",
        stderr(&run)
    );
    assert!(
        !home.join("runs").exists(),
        "the refused run left its directory behind"
    );
    assert!(
        yunta_testkit::pool_checkouts(&home).is_empty(),
        "the refused run took a checkout"
    );
    assert!(
        git_output(&repo, &["branch", "--list", "yunta/run/*"]).is_empty(),
        "the refused run left a run branch"
    );
}

#[test]
fn a_workflow_needing_agents_is_refused_before_creating_any_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    // `adapter:` isn't a closed enum in the schema — any name `runners:`
    // declares that `real_adapters` doesn't recognize is exactly "an
    // adapter this binary can't run." Both real adapters this binary
    // does build (`claude-code`; `codex`) are ruled out on
    // purpose, so this can't accidentally start passing once a third
    // one lands.
    write(
        &repo.join(".yunta/config.yaml"),
        r#"
runners:
  executor:
    - { adapter: some-future-cli, model: some-model }
"#,
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: needs-agent
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr
            .lines()
            .any(|l| l == ".yunta/tests/ and run `yunta test`."),
        "the refusal's closing line directs the user to the mock adapter via `yunta test`: {stderr}"
    );
    assert!(
        stderr.contains("`claude-code`") && stderr.contains("`codex`"),
        "the refusal names every adapter this binary builds: {stderr}"
    );
    assert!(
        !home.join("runs").exists(),
        "no run must be created when the refusal happens up front"
    );
}

#[test]
fn an_adapter_that_reports_itself_unhealthy_without_saying_why_is_refused_by_name_alone() {
    // A CLI whose `--version` exits non-zero writing nothing is the
    // shape a probe has no diagnostic for. The refusal lists the
    // adapter and stops: a colon with nothing behind it would promise
    // the reader a reason the CLI never gave.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let silent = repo.join("silent-cli.sh");
    write(&silent, "#!/bin/sh\nexit 1\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&silent).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&silent, perms).unwrap();
    }

    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            r#"
runners:
  executor:
    - {{ adapter: claude-code, model: some-model }}
adapters:
  claude-code:
    binary: {binary}
"#,
            binary = silent.display()
        ),
    );
    write(
        &repo.join("wf.yaml"),
        r#"
name: unhealthy-adapter
nodes:
  - id: implement
    kind: prompt
    runner: executor
    prompt: "Do the thing."
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    assert_eq!(
        stderr(&run).trim_end(),
        "error: adapter health check failed (run `yunta doctor` for detail): 1 error\n  \
         claude-code"
    );
}

// --- `--input` ---------------------------------------------------------

#[test]
fn an_input_s_default_is_used_when_input_is_not_given_on_the_command_line() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: greet
inputs:
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "test '{{inputs.greeting}}' = 'hola'"
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn an_explicit_input_flag_overrides_the_default() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: greet
inputs:
  greeting:
    type: string
    default: hola
nodes:
  - id: only
    kind: bash
    run: "test '{{inputs.greeting}}' = 'bonjour'"
"#,
    );

    let run = yunta_in!(
        &repo,
        &home,
        &["run", "wf.yaml", "--input", "greeting=bonjour"]
    );
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&run),
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn a_missing_required_input_refuses_before_creating_any_run() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        r#"
name: needs-idea
inputs:
  idea:
    type: string
    required: true
nodes:
  - id: only
    kind: bash
    run: "true"
"#,
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(!run.status.success());
    assert_eq!(
        unwrapped(&String::from_utf8_lossy(&run.stderr)),
        "error: input `idea` is required and has no default; pass `--input idea=...`",
        "the refusal names the missing input and how to supply it"
    );
    assert!(
        !home.join("runs").exists(),
        "no run must be created when input resolution fails"
    );
}

#[test]
fn mode_is_refused_since_modes_have_no_schema_yet() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join("wf.yaml"),
        "name: only-node\nnodes:\n  - id: only\n    kind: bash\n    run: \"true\"\n",
    );

    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--mode", "ship"]);
    assert!(!run.status.success());
    assert_eq!(
        unwrapped(&String::from_utf8_lossy(&run.stderr)),
        "error: workflow `only-node` declares no mode `ship`; declared modes: \
         (none: this workflow declares no modes)",
        "the refusal names the unknown mode and that the workflow declares none"
    );
}

#[test]
fn yunta_check_refuses_a_composition_cycle() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/workflows/a.yaml"),
        "name: a\nnodes:\n  - { id: sub, kind: workflow, use: b }\n",
    );
    write(
        &repo.join(".yunta/workflows/b.yaml"),
        "name: b\nnodes:\n  - { id: sub, kind: workflow, use: a }\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: parent\nnodes:\n  - { id: top, kind: workflow, use: a }\n",
    );

    let output = yunta_in!(&repo, &home, &["check", "wf.yaml"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("workflow composition cycle: a -> b -> a"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_second_run_over_max_concurrent_runs_is_refused_while_one_is_paused() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "limits:\n  max_concurrent_runs: 1\n",
    );
    // A bash node that fails with no on_failure pauses the run, leaving
    // it non-terminal.
    write(
        &repo.join("wf.yaml"),
        "name: pauser\nnodes:\n  - id: boom\n    kind: bash\n    run: \"false\"\n",
    );

    // A paused run exits non-zero (it needs attention) but leaves its
    // slot occupied — that's the state the second invocation must see.
    let first = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        stdout(&first).contains("needs you"),
        "got: {}",
        stdout(&first)
    );

    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        !second.status.success(),
        "the second run must be refused while the first is paused"
    );
    let err = String::from_utf8_lossy(&second.stderr);
    assert!(
        err.contains("max_concurrent_runs"),
        "the refusal must name the limit: {err}"
    );
    assert!(
        err.contains("resume") || err.contains("cancel"),
        "the refusal must say what to do about it: {err}"
    );
}

#[test]
fn a_finished_run_never_counts_against_max_concurrent_runs() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    write(
        &repo.join(".yunta/config.yaml"),
        "limits:\n  max_concurrent_runs: 1\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: ok\nnodes:\n  - id: fine\n    kind: bash\n    run: \"true\"\n",
    );

    let first = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(first.status.success());
    assert!(stdout(&first).contains("finished"));

    let second = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    assert!(
        second.status.success(),
        "a finished run holds no slot: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}

#[test]
fn every_command_that_opens_a_run_refuses_a_missing_one_with_the_same_sentence() {
    // A person who mistyped a run id gets one answer, not eight — and
    // the answer names every root this binary looked under, so they can
    // see where it did look.
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let ghost = "01GHOSTGHOSTGHOSTGHOSTGHOST";
    let mut said: Vec<(&str, String)> = Vec::new();
    for command in [
        vec!["status", ghost],
        vec!["stats", ghost],
        vec!["receipt", ghost],
        vec!["cancel", ghost],
        vec!["resume", ghost],
        vec!["resolve-gate", ghost, "approve"],
    ] {
        let name = command[0];
        let out = yunta_in!(&repo, &home, &command);
        assert!(
            !out.status.success(),
            "`{name}` must refuse a run it has no record of"
        );
        let text = stderr(&out);
        let line = text
            .lines()
            .find(|line| line.contains("no run"))
            .unwrap_or_else(|| panic!("`{name}` says which run it could not find: {text}"))
            .to_string();
        assert!(
            text.lines()
                .any(|line| line.trim_start().starts_with("looked in ") && line.contains("runs")),
            "`{name}` says where this binary looked for it: {text}"
        );
        said.push((name, line));
    }

    let (first_name, first) = &said[0];
    for (name, line) in &said[1..] {
        assert_eq!(
            line, first,
            "`{name}` and `{first_name}` say the same thing about a run neither has"
        );
    }
    assert!(first.contains(ghost), "the id the person typed: {first}");
}
