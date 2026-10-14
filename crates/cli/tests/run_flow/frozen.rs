//! What a run refuses before it exists or a resume takes anything: a pinned
//! adapter a runner lacks, a pack path that leaves its pack, and a mock run
//! that predates frozen adapters. A paused mock run goes on with the fixture
//! it was born with, and the log shows it.

use std::collections::HashSet;

use super::*;

/// A pack path that leaves its own pack is refused when the run is created,
/// naming the pack and the path, and no run directory is made.
#[test]
fn a_pack_path_that_leaves_its_pack_is_refused_before_the_run_exists() {
    let (_root, repo, home) = plain_repo();
    write(
        &repo.join(".yunta/packs/pub/tools/pack.yaml"),
        "publisher: pub\nname: tools\nversion: 1.0.0\ndeclares:\n  permissions: read-only\ncontents:\n  workflows: [\"../../../outside.yaml\"]\n",
    );
    write(
        &repo.join(".yunta/outside.yaml"),
        "name: outside\nnodes:\n  - { id: x, kind: bash, run: \"true\" }\n",
    );
    write(
        &repo.join("wf.yaml"),
        "name: parent\nnodes:\n  - { id: call, kind: workflow, use: pub/outside }\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "pack"]);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml"]);
    let said = format!("{}{}", stdout(&run), stderr(&run));
    assert!(
        !run.status.success()
            && said.contains(
                "pack `pub/tools` declares `../../../outside.yaml`, which leaves the pack's own directory"
            ),
        "{said}"
    );
    assert!(
        !home.join("runs").exists()
            || std::fs::read_dir(home.join("runs"))
                .unwrap()
                .next()
                .is_none()
    );
}

/// A workflow a pack declares and the run names by `publisher/name` is refused
/// when the run is created, naming the pack and the path that leaves it, and
/// no run directory is made.
#[test]
fn a_pack_workflow_that_leaves_its_pack_is_refused_when_run_by_name() {
    let (_root, repo, home) = plain_repo();
    write(
        &repo.join(".yunta/packs/pub/tools/pack.yaml"),
        "publisher: pub\nname: tools\nversion: 1.0.0\ndeclares:\n  permissions: read-only\ncontents:\n  workflows: [\"../../../outside.yaml\"]\n",
    );
    write(
        &repo.join(".yunta/outside.yaml"),
        "name: outside\nnodes:\n  - { id: x, kind: bash, run: \"true\" }\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "pack"]);
    let run = yunta_in!(&repo, &home, &["run", "pub/outside"]);
    let said = format!("{}{}", stdout(&run), stderr(&run));
    assert!(
        !run.status.success()
            && said.contains("pub/tools")
            && said.contains("../../../outside.yaml"),
        "{said}"
    );
    assert!(
        !home.join("runs").exists()
            || std::fs::read_dir(home.join("runs"))
                .unwrap()
                .next()
                .is_none()
    );
}

/// The workflow a gate parks between two prompt nodes, so a mock run pauses
/// with its first session behind it and its second still to run.
const GATED_MOCK_WORKFLOW: &str = "name: paused\nnodes:\n  - { id: first, kind: prompt, runner: executor, prompt: \"one\" }\n  - { id: approve, kind: gate, assignee: lead, depends_on: [first] }\n  - { id: second, kind: prompt, runner: executor, prompt: \"two\", depends_on: [approve] }\n";

/// One scripted session per prompt node in `GATED_MOCK_WORKFLOW`. A fixture
/// in the `sessions:` form names the CLI it stands for, so it carries a dialect.
const TWO_SESSION_FIXTURE: &str = "dialect: claude-code\nsessions:\n  - outcome: { type: completed, summary: one }\n  - outcome: { type: completed, summary: two }\n";

/// What the claude stub streams for a real session. Such a session would
/// finish the run too, so the argv the stub records is what tells them apart.
const CLAUDE_STUB_LINES: &str = r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}
{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":3,"output_tokens":2}}
"#;

/// A repo with an initial commit and a config whose `executor` runner names
/// claude-code at the stub.
fn gated_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (root, repo, home) = plain_repo();
    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            "runners:\n  executor:\n    - {{ adapter: claude-code, model: m }}\nadapters:\n  claude-code:\n    binary: {}\n",
            claude_code_stub().display()
        ),
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "config"]);
    (root, repo, home)
}

/// `gated_repo`, plus a runner `codex_only` whose only candidate is on codex.
fn override_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (root, repo, home) = gated_repo();
    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            "runners:\n  executor:\n    - {{ adapter: claude-code, model: m }}\n  codex_only:\n    - {{ adapter: codex, model: m }}\nadapters:\n  claude-code:\n    binary: {}\n  codex:\n    binary: {}\n",
            claude_code_stub().display(),
            codex_stub().display()
        ),
    );
    git(&repo, &["commit", "-q", "-am", "runners"]);
    (root, repo, home)
}

/// A repo with an initial commit and no configuration; the tempdir keeps it
/// alive, and the state home sits under it.
fn plain_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let home = root.path().join("state");
    (root, repo, home)
}

/// A pinned adapter that a runner has no candidate on refuses the run
/// before it is created, naming the node and the runner.
#[test]
fn a_pinned_adapter_missing_from_a_runner_is_refused_before_the_run_exists() {
    let (_root, repo, home) = override_repo();
    write(
        &repo.join("wf.yaml"),
        "name: pinned\nnodes:\n  - { id: a, kind: prompt, runner: codex_only, prompt: \"x\" }\n",
    );
    let run = yunta_in!(
        &repo,
        &home,
        &["run", "wf.yaml", "--adapter", "claude-code"]
    );
    let said = format!("{}{}", stdout(&run), stderr(&run));
    assert!(
        !run.status.success() && said.contains("node `a` runs on runner `codex_only`"),
        "{said}"
    );
    assert!(
        !home.join("runs").exists()
            || std::fs::read_dir(home.join("runs"))
                .unwrap()
                .next()
                .is_none()
    );
}

/// The events a run's log holds, in the order they were written.
fn run_log(run_dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(run_dir.join("events.jsonl"))
        .expect("the run keeps its log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line of the log is an event"))
        .collect()
}

/// The `node_finished` event that closed the node `id`.
fn node_finished(log: &[serde_json::Value], id: &str) -> serde_json::Value {
    log.iter()
        .find(|event| event["kind"] == "node_finished" && event["node_id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("`{id}` finished in the log"))
}

/// The session ids the log's `agent_session_opened` events name, in order.
fn opened_sessions(log: &[serde_json::Value]) -> Vec<&str> {
    log.iter()
        .filter(|event| event["kind"] == "agent_session_opened")
        .filter_map(|event| event["session_id"].as_str())
        .collect()
}

/// Whether the run's forensic log, exported once its last event is written,
/// records the run's finish.
fn log_names_finish(run_dir: &Path) -> bool {
    std::fs::read_to_string(run_dir.join("events.jsonl"))
        .is_ok_and(|text| text.contains("\"run_finished\""))
}

/// The mock's claims file in a run's directory: `None` while the file is
/// absent, which is before any session of the run has claimed a script.
fn mock_claims(run_dir: &Path) -> Option<String> {
    match std::fs::read_to_string(run_dir.join(yunta_adapters::MOCK_CLAIMS_FILE)) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("the run's claims file cannot be read: {error}"),
    }
}

/// A mock run paused at a gate finishes on its fixture once the gate is
/// answered: no real CLI runs although the config names one, its second node
/// ends with the fixture's second script, and each session it opens names an
/// id of its own.
#[tokio::test]
async fn a_paused_mock_run_resumes_on_its_fixture_not_a_real_cli() {
    let (root, repo, home) = gated_repo();
    write(&repo.join("wf.yaml"), GATED_MOCK_WORKFLOW);
    write(&repo.join("fixture.yaml"), TWO_SESSION_FIXTURE);
    // A real session that would finish the run too: only the argv it records tells them apart.
    write(&repo.join(".claude-stub-lines.jsonl"), CLAUDE_STUB_LINES);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "stub lines"]);
    let args = root.path().join("claude-args.txt");
    let yunta = |argv: &[&str]| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
        hermetic(&mut command, &repo, &home);
        command.args(argv).env("CLAUDE_STUB_ARGS_FILE", &args);
        CliChild::spawn(command, None)
            .expect("yunta spawns")
            .wait_with_output()
            .expect("yunta finishes")
    };
    let parked = yunta(&[
        "run",
        "wf.yaml",
        "--adapter",
        "mock",
        "--fixture",
        "fixture.yaml",
    ]);
    assert_eq!(parked.status.code(), Some(3));
    let id = only_run_id(&home);
    assert!(yunta(&["resolve-gate", &id, "approve"]).status.success());
    // The forensic log is exported once the run's last event is written, a
    // moment after the status page turns finished: the wait reads the export.
    let run_dir = home.join("runs").join(&id);
    yunta_testkit::wait_until_async(
        || std::future::ready(log_names_finish(&run_dir)),
        || "the run's log never recorded its finish".into(),
    )
    .await;
    assert!(!args.exists(), "a mock run never opens a real session");
    let log = run_log(&run_dir);
    assert_eq!(node_finished(&log, "second")["outcome"], "two");
    let opened = opened_sessions(&log);
    let distinct: HashSet<&str> = opened.iter().copied().collect();
    assert_eq!(opened.len(), 2, "one session per prompt node: {opened:?}");
    assert_eq!(
        distinct.len(),
        opened.len(),
        "a session id was named twice: {opened:?}"
    );
}

/// A mock run whose manifest has no adapter, but whose log opened a mock
/// session, predates frozen adapters: resume refuses it, naming the mock. The
/// refusal opens no session, so the claims the run kept are the same after it.
#[test]
fn a_mock_run_from_before_frozen_adapters_is_refused_on_resume() {
    let (_root, repo, home) = gated_repo();
    write(&repo.join("wf.yaml"), GATED_MOCK_WORKFLOW);
    write(&repo.join("fixture.yaml"), TWO_SESSION_FIXTURE);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "workflow"]);
    let parked = yunta_in!(
        &repo,
        &home,
        &[
            "run",
            "wf.yaml",
            "--adapter",
            "mock",
            "--fixture",
            "fixture.yaml"
        ]
    );
    assert_eq!(parked.status.code(), Some(3));
    let id = only_run_id(&home);
    let run_dir = home.join("runs").join(&id);
    let claims_before = mock_claims(&run_dir);
    // A mock run written before frozen adapters has no `adapter` in its manifest:
    // drop the key this run wrote, if it wrote one, and the manifest is that run's.
    let manifest = run_dir.join("manifest.yaml");
    let text = std::fs::read_to_string(&manifest).expect("the run wrote its manifest");
    let mut document: yunta_core::yaml::Value =
        yunta_core::yaml::parse(&text).expect("the manifest is YAML");
    document
        .as_mapping_mut()
        .expect("a manifest is a mapping")
        .remove("adapter");
    let rewritten = yunta_core::yaml::to_string(&document).expect("the manifest serializes");
    std::fs::write(manifest, rewritten).expect("the manifest is written back");
    let resumed = yunta_in!(&repo, &home, &["resume", id.as_str()]);
    let said = format!("{}{}", stdout(&resumed), stderr(&resumed));
    assert!(!resumed.status.success() && said.contains("mock"), "{said}");
    assert_eq!(
        mock_claims(&run_dir),
        claims_before,
        "the refused resume opened no session: {said}"
    );
}

/// A pinned adapter that no runner of the config names is refused before the
/// run exists, and the refusal lists the adapters the config runs, which is
/// only what it names, not every adapter this binary has.
#[test]
fn an_unbuilt_pin_is_refused_naming_the_adapters_the_config_runs() {
    let (_root, repo, home) = gated_repo();
    write(
        &repo.join("wf.yaml"),
        "name: bash\nnodes:\n  - { id: a, kind: bash, run: \"true\" }\n",
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "workflow"]);
    let run = yunta_in!(&repo, &home, &["run", "wf.yaml", "--adapter", "codex"]);
    let said = format!("{}{}", stdout(&run), stderr(&run));
    assert!(!run.status.success(), "{said}");
    assert!(
        said.lines().any(|line| line
            .trim_end()
            .ends_with("this binary can run: claude-code")),
        "{said}"
    );
    assert!(!home.join("runs").exists(), "no run was created: {said}");
}
