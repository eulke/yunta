//! The cases that need a process of their own: each one kills, interrupts or
//! reads the stderr of a live CLI, so only the real subjects run them.

use std::time::{Duration, Instant};

use yunta_core::port::{AgentSession, SessionRequest};
use yunta_core::{AdapterSettings, Pid, Secret};
use yunta_testkit_core::adapter::{child_pid_fifo, drain, drain_for_exit};
use yunta_testkit_core::adapter::{grandchild_pid, wait_until_gone, write_lines};

use super::cases::fresh;
use super::subject::Subject;

/// Far longer than a session that honours its interrupt takes to leave.
const GRACE: Duration = Duration::from_secs(30);

/// Far below `GRACE`, far above the time a process takes to exit on its signal.
const LEAVES_WITHIN: Duration = Duration::from_secs(5);

/// Sets one knob of the scripted CLI in the session's environment.
fn knob(subject: &dyn Subject, req: &mut SessionRequest, name: &str, value: impl ToString) {
    let key = subject.stub_var(name);
    req.env.plain.insert(key, value.to_string());
}

/// Spawns a session of the subject's CLI and returns it live.
async fn spawn_live(subject: &dyn Subject, req: SessionRequest) -> Box<dyn AgentSession> {
    let adapter = subject.adapter(AdapterSettings::default());
    adapter.spawn(req).await.unwrap()
}

/// Killing a session takes its whole process tree down, grandchildren included.
pub async fn kill_terminates_the_whole_process_tree_including_grandchildren(subject: &dyn Subject) {
    let (dir, mut req) = fresh();
    let pid_file = child_pid_fifo(dir.path());
    knob(subject, &mut req, "CHILD_PID_FILE", pid_file.display());
    knob(subject, &mut req, "HANG", 1);
    let mut session = spawn_live(subject, req).await;
    let grandchild: i32 = grandchild_pid(&pid_file).await.parse().unwrap();
    session.kill().await.unwrap();
    wait_until_gone(Pid::try_from(grandchild).unwrap()).await;
}

/// A dead session's stderr tail never carries a value from its environment.
pub async fn a_dead_sessions_stderr_tail_never_carries_a_value_from_its_env(subject: &dyn Subject) {
    let (dir, mut req) = fresh();
    let token = "s3cr3t-token-value";
    let secret = Secret::new(token.to_string());
    let secrets = &mut req.env.secret;
    secrets.insert("YUNTA_RUN_TOOLS_TOKEN".to_string(), secret);
    let line = format!("auth failed for bearer {token}");
    let said = write_lines(dir.path(), "stderr.txt", &[line.as_str()]);
    knob(subject, &mut req, "STDERR_FILE", said.display());
    knob(subject, &mut req, "EXIT", 1);
    let session = spawn_live(subject, req).await;
    let (_, exit) = drain_for_exit(session).await.unwrap();
    let tail = exit.expect("the session ended with a process").stderr_tail;
    assert_eq!(tail, ["auth failed for bearer [redacted]"]);
}

/// The prompt reaches the CLI on stdin, whole, and never on its argument list.
pub async fn prompt_travels_by_stdin_never_argv(subject: &dyn Subject) {
    let (dir, mut req) = fresh();
    let prompt = "the whole brief, with a --flag-looking line";
    req.prompt = prompt.to_string();
    let args = dir.path().join("args.txt");
    let stdin = dir.path().join("stdin.txt");
    knob(subject, &mut req, "ARGS_FILE", args.display());
    knob(subject, &mut req, "STDIN_FILE", stdin.display());
    drain(spawn_live(subject, req).await).await;
    let argv = tokio::fs::read_to_string(&args).await.unwrap();
    assert!(!argv.contains("the whole brief"), "{argv}");
    let read = tokio::fs::read_to_string(&stdin).await.unwrap();
    assert_eq!(read, prompt);
}

/// An interrupted session that leaves ends its grace at once, rather than sitting it out.
pub async fn an_interrupted_session_that_leaves_ends_its_grace_at_once(subject: &dyn Subject) {
    let (dir, mut req) = fresh();
    let pid_file = child_pid_fifo(dir.path());
    knob(subject, &mut req, "CHILD_PID_FILE", pid_file.display());
    knob(subject, &mut req, "LINGER", 1);
    let mut session = spawn_live(subject, req).await;
    grandchild_pid(&pid_file).await;
    session.interrupt().await.unwrap();
    let started = Instant::now();
    session.allow_exit(GRACE).await;
    let waited = started.elapsed();
    session.kill().await.unwrap();
    assert!(waited < LEAVES_WITHIN, "waited {waited:?}");
}
