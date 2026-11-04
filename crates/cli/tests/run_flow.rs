//! End-to-end CLI flows: `yunta run` on a bash-only workflow (no agent
//! adapter needed), `status` over its log, `resume` idempotence,
//! `yunta test` driving a workflow with the mock adapter from a case
//! file (the designed home for mock fixtures), and `yunta run`
//! actually spawning the real `claude-code` adapter against a
//! scripted fake `claude` binary — no network, no cost.

use std::path::{Path, PathBuf};

use yunta_core::process::signal::{liveness, signal_group, signal_process, Liveness, Signal};
use yunta_core::Pid;
use yunta_testkit::{
    git, git_output, hermetic, init_repo, run_id_from, stderr, stdout, unwrapped, wait_for,
    wait_until, write, yunta_at, yunta_in, Checkout, CliChild, ProcessGroupCleanup,
};

#[path = "run_flow/frozen.rs"]
mod frozen;

#[path = "run_flow/detached_cancel.rs"]
mod detached_cancel;

#[path = "run_flow/running.rs"]
mod running;

#[path = "run_flow/refusals.rs"]
mod refusals;

#[path = "run_flow/yunta_test.rs"]
mod yunta_test;

#[path = "run_flow/reaching.rs"]
mod reaching;

#[path = "run_flow/checkouts.rs"]
mod checkouts;

#[path = "run_flow/resuming.rs"]
mod resuming;

#[path = "run_flow/reading.rs"]
mod reading;

#[path = "run_flow/stopping.rs"]
mod stopping;

#[path = "run_flow/detaching.rs"]
mod detaching;

#[path = "run_flow/answering.rs"]
mod answering;

#[path = "run_flow/interrupts.rs"]
mod interrupts;

fn claude_code_stub() -> PathBuf {
    yunta_testkit_core::stubs::claude_code()
}

/// A workflow whose second node fails, so its run pauses after the first.
const PAUSES_ON_A_FAILING_NODE: &str = r#"
name: pauses-on-a-failing-node
nodes:
  - id: setup
    kind: bash
    run: "echo hello > marker.txt"
  - id: always-fails
    kind: bash
    depends_on: [setup]
    run: "touch attempt-marker.txt && exit 1"
"#;

/// True once `marker` exists and holds non-whitespace content — the point
/// at which the shell has finished writing it, not merely created it. A
/// poll for existence alone can observe the file in the window between
/// creation and the write completing.
fn marker_written(marker: &Path) -> bool {
    std::fs::read_to_string(marker)
        .map(|content| !content.trim().is_empty())
        .unwrap_or(false)
}

/// Spawns `yunta run` detached and waits until the given file is written —
/// the bash node's own signal that it is really running.
fn spawn_run_until(repo: &Path, home: &Path, marker: &Path) -> CliChild {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
    hermetic(&mut command, repo, home);
    command.args(["run", "wf.yaml"]);
    let child = CliChild::spawn(command, None).unwrap();
    wait_until(
        || marker_written(marker),
        || "the bash node never started".into(),
    );
    child
}

/// The process state `ps` reports for `pid`: empty once the process is
/// gone, `Z` while it is a zombie nobody reaped.
fn process_state(pid: &str) -> String {
    let output = std::process::Command::new("ps")
        .args(["-o", "state=", "-p", pid])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The one run id under `home/runs` — usable before the run's own
/// process has printed anything.
fn only_run_id(home: &Path) -> String {
    let mut entries: Vec<String> = std::fs::read_dir(home.join("runs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one run: {entries:?}");
    entries.pop().unwrap()
}

/// The pid of a child this test spawned.
trait ChildPid {
    fn child_pid(&self) -> Pid;
}

impl ChildPid for std::process::Child {
    fn child_pid(&self) -> Pid {
        Pid::try_from(self.id()).expect("a spawned child has a positive pid")
    }
}

impl ChildPid for CliChild {
    fn child_pid(&self) -> Pid {
        self.pid()
    }
}

fn pid_of(child: &impl ChildPid) -> Pid {
    child.child_pid()
}

/// A pid a run wrote for the test to read back.
fn parse_pid(text: &str) -> Pid {
    text.trim()
        .parse::<u32>()
        .ok()
        .and_then(|raw| Pid::try_from(raw).ok())
        .unwrap_or_else(|| panic!("`{text}` is not a pid"))
}

fn codex_stub() -> PathBuf {
    yunta_testkit_core::stubs::codex()
}

/// A repo whose only runner lists `codex` first and `claude-code`
/// second, each a stub that records its last invocation's arguments in
/// the file its `*_STUB_ARGS_FILE` names.
struct Override {
    _root: tempfile::TempDir,
    repo: PathBuf,
    home: PathBuf,
    claude_args: PathBuf,
    codex_args: PathBuf,
}

impl Override {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        write(
            &repo.join(".yunta/config.yaml"),
            &format!(
                r#"
runners:
  executor:
    - {{ adapter: codex, model: codex-model }}
    - {{ adapter: claude-code, model: claude-model }}
adapters:
  codex:
    binary: {codex}
  claude-code:
    binary: {claude}
secrets: [CLAUDE_STUB_ARGS_FILE, CODEX_STUB_ARGS_FILE]
"#,
                codex = codex_stub().display(),
                claude = claude_code_stub().display()
            ),
        );
        write(
            &repo.join("wf.yaml"),
            "name: override\nnodes:\n  - id: implement\n    kind: prompt\n    runner: executor\n    prompt: \"Do the thing.\"\n",
        );
        write(
            &repo.join(".claude-stub-lines.jsonl"),
            &format!(
                "{}\n{}\n",
                r#"{"type":"system","subtype":"init","session_id":"sess-cli","model":"claude-sonnet-5"}"#,
                r#"{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":3,"output_tokens":2}}"#,
            ),
        );
        git(&repo, &["add", ".claude-stub-lines.jsonl"]);
        git(&repo, &["commit", "-q", "-m", "stub fixture"]);
        Self {
            home: root.path().join("state"),
            claude_args: root.path().join("claude-args.txt"),
            codex_args: root.path().join("codex-args.txt"),
            repo,
            _root: root,
        }
    }

    fn yunta(&self, args: &[&str]) -> std::process::Output {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
        hermetic(&mut command, &self.repo, &self.home);
        command
            .args(args)
            .env("CLAUDE_STUB_ARGS_FILE", &self.claude_args)
            .env("CODEX_STUB_ARGS_FILE", &self.codex_args);
        CliChild::spawn(command, None)
            .expect("the yunta binary spawns")
            .wait_with_output()
            .expect("yunta finishes within the test deadline")
    }

    /// Whether the session ran on `claude-code` and never on `codex`.
    /// Both stubs record every invocation, the health probe
    /// (`--version`) included: a session is the invocation that is not
    /// the probe.
    fn ran_on_claude_only(&self) -> (bool, bool) {
        let ran_a_session = |args: &Path| {
            std::fs::read_to_string(args).is_ok_and(|text| !text.contains("--version"))
        };
        (
            ran_a_session(&self.claude_args),
            ran_a_session(&self.codex_args),
        )
    }
}

/// Spawns the binary in `project` with its stderr on `log`, for a test
/// that signals it while it runs.
fn spawn_yunta(project: &Checkout, args: &[&str], log: PathBuf) -> CliChild {
    let mut command = project.command(Path::new(env!("CARGO_BIN_EXE_yunta")));
    command.args(args);
    CliChild::spawn(command, Some(&log)).expect("the binary runs")
}

/// A workflow whose one node announces itself beside the checkout and
/// then holds, so a test asserts against a run that is provably still
/// working. The markers live outside the tree: `isolation: none`
/// refuses a dirty one.
const HOLDS_OPEN: &str = "name: held\nnodes:\n  - id: work\n    kind: bash\n    run: \"echo up > \
                          ../started.txt; tail -f /dev/null\"\n";

/// A detached run of [`HOLDS_OPEN`], already at its node: the id it was
/// given and the engine its registry names.
fn detached_run_holding(project: &Checkout, root: &Path) -> (String, Pid) {
    let bin = Path::new(env!("CARGO_BIN_EXE_yunta"));
    let detached = project.run(bin, &["run", "wf.yaml", "--detach"]);
    assert!(detached.status.success(), "{}", stderr(&detached));
    let run_id = run_id_from(&detached);
    wait_until(
        || marker_written(&root.join("started.txt")),
        || "the detached run never reached the node".into(),
    );
    let engine = engine_pid_of(
        &yunta_testkit::runs_root(&project.home)
            .join(yunta_testkit::full_run_id(&project.home, &run_id)),
    );
    (run_id, engine)
}

/// The pid `engine.json` names as the live engine of the run at
/// `run_dir`.
fn engine_pid_of(run_dir: &Path) -> Pid {
    let registry = std::fs::read_to_string(run_dir.join("scratch/engine.json"))
        .expect("a live run writes its registry");
    let pid = registry
        .split("\"engine_pid\":")
        .nth(1)
        .and_then(|rest| {
            rest.split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
        })
        .expect("the registry names the engine's pid");
    parse_pid(pid)
}
