//! A session ends when its CLI says its turn ended, and nothing its CLI
//! started outlives it: whole runs through the scripted CLIs, as a person
//! runs one.

use std::path::Path;

use yunta_core::process::signal::{signal_process, Signal};
use yunta_core::Pid;
use yunta_testkit::{
    full_run_id, git, hermetic, init_repo, run_id_from, runs_root, stdout, wait_until, write,
    yunta_in, CliChild,
};
use yunta_testkit_core::adapter::{child_pid_fifo, recorded_pid, wait_until_gone};

/// Each scripted CLI, its knobs' prefix, and the lines of a session that
/// opens, says something and ends its turn.
const SPEAKERS: [(&str, &str, &[&str]); 2] = [
    (
        "claude-code",
        "CLAUDE_STUB",
        &[
            r#"{"type":"system","subtype":"init","session_id":"sess-ends","model":"m"}"#,
            r#"{"type":"assistant","message":{"id":"msg_0","content":[{"type":"text","text":"Started it."}]}}"#,
            r#"{"type":"result","is_error":false,"result":"done","usage":{"input_tokens":3,"output_tokens":2}}"#,
        ],
    ),
    (
        "codex",
        "CODEX_STUB",
        &[
            r#"{"type":"thread.started","thread_id":"thread-ends"}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Started it."}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":0,"output_tokens":2}}"#,
        ],
    ),
];

/// Writes `repo` as a project whose one node's session stands on `adapter`,
/// played by its stub, and commits it. The stub prints `lines`.
fn project(repo: &Path, adapter: &str, lines: &[&str]) {
    std::fs::create_dir_all(repo).unwrap();
    init_repo(repo);
    let binary = match adapter {
        "codex" => yunta_testkit_core::stubs::codex(),
        _ => yunta_testkit_core::stubs::claude_code(),
    };
    write(
        &repo.join(".yunta/config.yaml"),
        &format!(
            "defaults:\n  isolation: none\n  timeout_minutes: 1\nrunners:\n  executor:\n    - {{ adapter: {adapter}, model: m }}\nadapters:\n  {adapter}:\n    binary: {}\n",
            binary.display()
        ),
    );
    write(
        &repo.join("wf.yaml"),
        "name: ends\nnodes:\n  - { id: talk, kind: prompt, runner: executor, prompt: \"Work.\" }\n",
    );
    let file = match adapter {
        "codex" => ".codex-stub-lines.jsonl",
        _ => ".claude-stub-lines.jsonl",
    };
    write(&repo.join(file), &format!("{}\n", lines.join("\n")));
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "project"]);
}

/// What a stub starts in a process group of its own, under it.
const GROUPED: &str = "its command in a group of its own";

/// What a stub starts from a shell that leaves at once, so its parent is gone.
const ORPHANED: &str = "its orphaned command";

/// Processes a test started, killed when it ends, passed or not.
#[derive(Default)]
struct Started(Vec<Pid>);

impl Drop for Started {
    fn drop(&mut self) {
        for pid in &self.0 {
            // best-effort: a process already gone is what the test wanted.
            let _ = signal_process(*pid, Signal::SIGKILL);
        }
    }
}

/// The pid a stub's `command` writes to `fifo` once it runs. A stub that never
/// writes it fails the test with what the run under `home` left, since a run
/// that never reached its session says why only there.
async fn pid_in(command: &str, fifo: &Path, repo: &Path, home: &Path) -> Pid {
    let Some(pid) = recorded_pid(fifo).await else {
        // The stub makes its rendezvous with the command beside the fifo:
        // there, it started the command and waits on it.
        let waits = Path::new(&format!("{}.ready", fifo.display())).exists();
        panic!(
            "the stub never recorded the pid of {command} (the stub {} it); the run left:\n{}",
            if waits {
                "started it and waits on"
            } else {
                "never started"
            },
            left(repo, home)
        );
    };
    Pid::try_from(pid.parse::<u32>().unwrap()).unwrap()
}

/// What the one run under `home` left: what its detached process wrote, what
/// its registry names, and where its log says it stands.
fn left(repo: &Path, home: &Path) -> String {
    let Some(run) = std::fs::read_dir(runs_root(home))
        .ok()
        .and_then(|mut runs| runs.next())
        .and_then(Result::ok)
    else {
        return "no run directory".to_string();
    };
    let id = run.file_name().to_string_lossy().into_owned();
    let scratch = |name: &str| {
        std::fs::read_to_string(run.path().join("scratch").join(name))
            .unwrap_or_else(|error| format!("({error})"))
    };
    let status = yunta_in!(repo, home, &["status", &id]);
    format!(
        "detached.log:\n{}\nengine.json:\n{}\nyunta status {id}:\n{}{}",
        scratch("detached.log"),
        scratch("engine.json"),
        stdout(&status),
        String::from_utf8_lossy(&status.stderr)
    )
}

/// A Codex session that prints a retry notice and goes on finishes its node,
/// and its run, as Codex finishes the turn.
/// Reached by: `crates/adapters/src/codex/parse.rs`
#[test]
fn a_codex_session_that_retries_finishes() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    let home = root.path().join("state");
    project(
        &repo,
        "codex",
        &[
            r#"{"type":"thread.started","thread_id":"thread-ends"}"#,
            r#"{"type":"error","message":"Reconnecting... 2/5 (unexpected status 503 Service Unavailable)"}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"Done after the retry."}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":3,"cached_input_tokens":0,"output_tokens":2}}"#,
        ],
    );

    let ran = yunta_in!(&repo, &home, &["run", "wf.yaml", "--quiet"]);

    assert!(ran.status.success(), "{ran:?}");
    let id = stdout(&ran).trim().to_string();
    let status = yunta_in!(&repo, &home, &["status", &id, "--json"]);
    let document: serde_json::Value = serde_json::from_str(&stdout(&status)).unwrap();
    assert_eq!(document["outcome"], "finished", "{document}");
}

/// Through each scripted CLI, a session that ends its turn by itself leaves
/// nothing its CLI started: a command in a group of its own under the CLI,
/// and one whose parent left. The CLI stays up after its last line, as a
/// real CLI does while it cleans up.
/// Reached by: `crates/engine/src/task_cycle/dispatch.rs`, `crates/core/src/process/subprocess.rs`
#[tokio::test]
async fn no_command_outlives_a_session_that_ends_by_itself() {
    for (adapter, prefix, lines) in SPEAKERS {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let home = root.path().join("state");
        project(&repo, adapter, lines);
        let (under_dir, orphan_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let under = child_pid_fifo(under_dir.path());
        let orphan = child_pid_fifo(orphan_dir.path());
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_yunta"));
        hermetic(&mut command, &repo, &home);
        command
            .args(["run", "wf.yaml", "--quiet"])
            .env(format!("{prefix}_GROUP_CHILD"), &under)
            .env(format!("{prefix}_ORPHAN_CHILD"), &orphan)
            .env(format!("{prefix}_LINGER"), "1");
        let run = CliChild::spawn(command, None).unwrap();
        let mut started = Started::default();
        started.0.push(pid_in(GROUPED, &under, &repo, &home).await);
        started
            .0
            .push(pid_in(ORPHANED, &orphan, &repo, &home).await);

        let output = run.wait_with_output().unwrap();

        assert!(output.status.success(), "{adapter}: {output:?}");
        for pid in &started.0 {
            wait_until_gone(*pid).await;
        }
    }
}

/// How a run in the middle of its session is stopped from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// `yunta cancel`, its engine alive.
    Cancel,
    /// `yunta cancel`, once its engine was killed.
    CancelAfterCrash,
    /// `yunta close`, once its engine was killed.
    CloseAfterCrash,
}

/// Kills the engine driving run `id` once its registry names the session's
/// group, and waits until the engine is gone.
async fn kill_engine(home: &Path, id: &str) {
    let registry = runs_root(home).join(id).join("scratch").join("engine.json");
    let read = || -> Option<serde_json::Value> {
        serde_json::from_slice(&std::fs::read(&registry).ok()?).ok()
    };
    wait_until(
        || {
            read().is_some_and(|engine| {
                engine["process_groups"]
                    .as_array()
                    .is_some_and(|groups| !groups.is_empty())
            })
        },
        || "the engine never registered its session's group".into(),
    );
    let engine = read().unwrap();
    let pid =
        Pid::try_from(u32::try_from(engine["engine_pid"].as_u64().unwrap()).unwrap()).unwrap();
    signal_process(pid, Signal::SIGKILL).unwrap();
    wait_until_gone(pid).await;
}

/// Through each scripted CLI, a run whose session is in the middle of its
/// turn, holding a command in a group of its own and one whose parent left,
/// is stopped by `stop`, and then neither command runs.
async fn no_command_outlives(stop: Stop) {
    for (adapter, prefix, lines) in SPEAKERS {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let home = root.path().join("state");
        // The lines before the turn's end: the session stays open.
        project(&repo, adapter, &lines[..lines.len() - 1]);
        let (under_dir, orphan_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let under = child_pid_fifo(under_dir.path());
        let orphan = child_pid_fifo(orphan_dir.path());
        let said = under_dir.path().join("said");
        let knobs = [
            (format!("{prefix}_SAID_FILE"), said.display().to_string()),
            (format!("{prefix}_GROUP_CHILD"), under.display().to_string()),
            (
                format!("{prefix}_ORPHAN_CHILD"),
                orphan.display().to_string(),
            ),
            (format!("{prefix}_LINGER"), "1".to_string()),
        ];
        let detached = yunta_in!(&repo, &home, &["run", "wf.yaml", "--detach"], &knobs);
        assert!(detached.status.success(), "{adapter}: {detached:?}");
        let mut started = Started::default();
        started.0.push(pid_in(GROUPED, &under, &repo, &home).await);
        started
            .0
            .push(pid_in(ORPHANED, &orphan, &repo, &home).await);
        let id = full_run_id(&home, &run_id_from(&detached));
        // An engine killed while the CLI still writes takes the CLI with it,
        // by SIGPIPE, and its commands with no parent link to follow: that
        // is the case the mark answers for, and not this one.
        wait_until(
            || said.exists(),
            || format!("{adapter} never printed its lines"),
        );
        if stop != Stop::Cancel {
            kill_engine(&home, &id).await;
        }
        let verb = if stop == Stop::CloseAfterCrash {
            "close"
        } else {
            "cancel"
        };

        let stopped = yunta_in!(&repo, &home, &[verb, &id]);

        assert!(stopped.status.success(), "{adapter}, {verb}: {stopped:?}");
        for pid in &started.0 {
            wait_until_gone(*pid).await;
        }
    }
}

/// `yunta cancel` of a run whose engine lives leaves no command its
/// session's CLI started.
/// Reached by: `crates/engine/src/task_cycle/session.rs`, `crates/core/src/process/subprocess.rs`
#[tokio::test]
async fn cancel_with_its_engine_alive_leaves_no_command() {
    no_command_outlives(Stop::Cancel).await;
}

/// `yunta cancel` of a run whose engine was killed leaves no command its
/// session's CLI started.
#[tokio::test]
async fn cancel_after_its_engine_died_leaves_no_command() {
    no_command_outlives(Stop::CancelAfterCrash).await;
}

/// `yunta close` of a run whose engine was killed leaves no command its
/// session's CLI started.
#[tokio::test]
async fn close_after_its_engine_died_leaves_no_command() {
    no_command_outlives(Stop::CloseAfterCrash).await;
}
