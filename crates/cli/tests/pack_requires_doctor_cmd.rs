//! `yunta doctor` validating an installed pack's `requires:` against the
//! local config — end to end against the real
//! compiled binary.

use std::path::Path;

use yunta_testkit::{git, init_repo, stderr, stdout, yunta_in};

fn write_pack_with_requires(dir: &Path) {
    std::fs::create_dir_all(dir.join("workflows")).unwrap();
    std::fs::write(
        dir.join("pack.yaml"),
        "name: review-pack\n\
         publisher: acme\n\
         version: 1.0.0\n\
         requires:\n  \
           runners: [{ name: reviewer }]\n  \
           mcp_servers: [internal-docs]\n  \
           commands: [this-binary-almost-certainly-does-not-exist-anywhere]\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "v1"]);
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    write_pack_with_requires(&upstream);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);

    let home = root.path().join("state");
    (root, upstream, home)
}

#[test]
fn doctor_flags_a_pack_whose_requires_the_local_config_cannot_satisfy() {
    let (root, upstream, home) = setup();
    let repo = root.path().join("repo");

    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let doctor_out = yunta_in!(&repo, &home, &["doctor"]);
    assert!(!doctor_out.status.success());
    let text = stdout(&doctor_out);
    assert!(
        text.lines().any(|l| l == "pack acme/review-pack requires:"),
        "the pack's unmet requirements are reported under one header: {text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("  runner `reviewer` — ")),
        "the unresolvable runner is named: {text}"
    );
    assert!(
        text.lines().any(|l| l
            == "  mcp_server `internal-docs` — not defined under `mcp_servers:`; add it there"),
        "the undefined mcp_server is named: {text}"
    );
    assert!(
        text.lines().any(|l| l
            == "  command `this-binary-almost-certainly-does-not-exist-anywhere` — not found on PATH"),
        "the missing command is named: {text}"
    );
}

#[test]
fn doctor_is_silent_about_a_pack_with_no_unmet_requires() {
    let root = tempfile::tempdir().unwrap();
    let upstream = root.path().join("upstream");
    std::fs::create_dir_all(&upstream).unwrap();
    init_repo(&upstream);
    std::fs::create_dir_all(upstream.join("workflows")).unwrap();
    std::fs::write(
        upstream.join("pack.yaml"),
        "name: review-pack\npublisher: acme\nversion: 1.0.0\n\
         declares:\n  permissions: read-only\n  network: false\n  executors: []\n\
         contents:\n  workflows: [workflows/review.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        upstream.join("workflows/review.yaml"),
        "name: review\nnodes:\n  - id: noop\n    kind: bash\n    run: \"true\"\n",
    )
    .unwrap();
    git(&upstream, &["add", "."]);
    git(&upstream, &["commit", "-q", "-m", "v1"]);

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");

    let add_out = yunta_in!(&repo, &home, &["pack", "add", upstream.to_str().unwrap()]);
    assert!(add_out.status.success(), "{}", stderr(&add_out));

    let doctor_out = yunta_in!(&repo, &home, &["doctor"]);
    assert!(doctor_out.status.success(), "{}", stderr(&doctor_out));
    assert!(!stdout(&doctor_out).contains("requires"));
}

// --- `doctor --session` ---------------------------------------------------

/// A project whose runners all resolve to a `codex` that refuses
/// whatever it is given: it writes the refusal on stderr and exits
/// before its first line, which is what a CLI rejecting the
/// configuration this engine writes it actually does.
///
/// `probe()` sees none of that — the stub answers `--version` like the
/// real CLI — so a plain `doctor` calls it healthy and only a session
/// finds out.
fn a_project_whose_cli_dies(root: &Path, repo: &Path, runners: &str) {
    let said = root.join("stderr.txt");
    std::fs::write(&said, "url is not supported for stdio\n").unwrap();
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        format!(
            "runners:\n{runners}adapters:\n  codex:\n    binary: {stub}\nbaseline:\n  suite: \
             \"touch {measured}\"\nsecrets: [CODEX_STUB_STDERR_FILE, CODEX_STUB_EXIT]\n",
            stub = yunta_testkit_core::stubs::codex().display(),
            measured = root.join("measured.txt").display(),
        ),
    )
    .unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "project"]);
}

/// Runs `yunta doctor` with the stub armed to die, and hands back what
/// it printed.
fn doctor_with(repo: &Path, home: &Path, root: &Path, args: &[&str]) -> std::process::Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, repo, home);
    command
        .args(args)
        .env("CODEX_STUB_STDERR_FILE", root.join("stderr.txt"))
        .env("CODEX_STUB_EXIT", "2")
        .output()
        .expect("the yunta binary runs")
}

#[test]
fn doctor_session_reports_a_binding_whose_cli_dies_at_startup_with_its_stderr() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    a_project_whose_cli_dies(
        root.path(),
        &repo,
        "  executor:\n    - { adapter: codex, model: codex-model }\n",
    );

    let plain = doctor_with(&repo, &home, root.path(), &["doctor"]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    assert!(
        stdout(&plain).contains("codex: healthy"),
        "a probe is the binary answering, and it does: {}",
        stdout(&plain)
    );

    let opened = doctor_with(&repo, &home, root.path(), &["doctor", "--session"]);
    let text = stdout(&opened);
    assert!(
        !opened.status.success(),
        "a binding no session opens on is something to act on: {text}"
    );
    assert!(
        text.contains("codex/codex-model (executor): session died"),
        "the binding is named, and so is every runner that reaches it: {text}"
    );
    assert!(
        text.contains("exited with code 2") && text.contains("url is not supported for stdio"),
        "with how the process went and what it said on its way out: {text}"
    );
}

#[test]
fn doctor_session_refuses_a_reply_without_the_questions_submission() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        format!("defaults:\n  runner: executor\nrunners:\n  executor:\n    - {{ adapter: codex, model: codex-model }}\nadapters:\n  codex:\n    binary: {}\nsecrets: [CODEX_STUB_LINES_FILE]\n", yunta_testkit_core::stubs::codex().display()),
    ).unwrap();
    std::fs::write(
        repo.join(".codex-stub-lines.jsonl"),
        "{\"type\":\"thread.started\",\"thread_id\":\"doctor-no-delivery\"}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}\n",
    ).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "project"]);

    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, &repo, &home);
    let output = command
        .args(["doctor", "--session"])
        .env(
            "CODEX_STUB_LINES_FILE",
            repo.join(".codex-stub-lines.jsonl"),
        )
        .output()
        .expect("doctor runs");
    let text = stdout(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("session opened, no questions document"),
        "{text}"
    );
}

#[test]
fn doctor_session_names_every_runner_that_reaches_a_binding() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    // One binding two runners reach, the second of them only when its
    // own first choice is down — and the same binding is worth as much
    // there as it is first.
    a_project_whose_cli_dies(
        root.path(),
        &repo,
        "  reviewer:\n    - { adapter: codex, model: codex-model }\n  \
         planner:\n    - { adapter: codex, model: other-model }\n    \
         - { adapter: codex, model: codex-model }\n",
    );

    let text = stdout(&doctor_with(
        &repo,
        &home,
        root.path(),
        &["doctor", "--session"],
    ));
    assert!(
        text.contains("codex/codex-model (planner fallback, reviewer)"),
        "each runner that reaches it, and which of them falls back to it: {text}"
    );
    assert!(
        text.contains("codex/other-model (planner)"),
        "and a binding held first is its own line: {text}"
    );
}

#[test]
fn doctor_session_never_measures_the_projects_baseline() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    a_project_whose_cli_dies(
        root.path(),
        &repo,
        "  executor:\n    - { adapter: codex, model: codex-model }\n",
    );

    doctor_with(&repo, &home, root.path(), &["doctor", "--session"]);
    assert!(
        !root.path().join("measured.txt").exists(),
        "a probe asks whether a session opens, never what the tree measured"
    );
}

#[test]
fn doctor_without_session_opens_none() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    a_project_whose_cli_dies(
        root.path(),
        &repo,
        "  executor:\n    - { adapter: codex, model: codex-model }\n",
    );

    let text = stdout(&doctor_with(&repo, &home, root.path(), &["doctor"]));
    assert!(
        !text.contains("session died"),
        "nothing was opened, so nothing died: {text}"
    );
    assert!(
        text.contains("no session opened"),
        "and the command says what it did not check: {text}"
    );
}
