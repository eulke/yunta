//! What a test of an adapter needs before it can assert anything: a
//! session request, the events a session produced, a scripted CLI's
//! output on disk, and a rendezvous with the process that CLI spawned.
//!
//! Each adapter's tests used to carry their own copy of these. One copy
//! is what keeps the three suites asking the same question — a request
//! that gains a field reaches every adapter's tests at once, and a
//! rendezvous that is a poll in one file and a fifo in another stops
//! being possible.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use yunta_core::fence::{Advice, Fence, FenceHook};

use futures::StreamExt;
use yunta_core::events::SessionExit;
use yunta_core::port::{AgentEvent, AgentSession, Budget, PermissionProfile, SessionRequest};
use yunta_core::process::signal::{liveness, Liveness};
use yunta_core::Pid;

/// A session request with nothing declared: the baseline a test varies
/// one field of, so what it asserts about is the field it set.
pub fn request(cwd: PathBuf) -> SessionRequest {
    let cwd_scratch = cwd.join(".yunta-scratch");
    SessionRequest {
        prompt: "do the thing".to_string(),
        cwd,
        model: None,
        agent: None,
        permissions: PermissionProfile::Edit,
        env: HashMap::new(),
        fence: Fence::everything(Vec::new(), Advice::ReportFinding),
        fence_hook: Some(FenceHook::new(PathBuf::from("yunta"))),
        budget: Budget::default(),
        adapter_settings: serde_json::Map::new(),
        skills: Vec::new(),
        run_tools_endpoint: None,
        artifact_dir: None,
        scratch_dir: cwd_scratch,
    }
}

/// Every event `session` produces, to the end of its stream.
pub async fn drain(mut session: Box<dyn AgentSession>) -> Vec<AgentEvent> {
    to_the_end(&mut session).await
}

/// The same, and how the session's process ended — the pair the engine
/// reads of a session whose stream said nothing terminal, so a test
/// reads both without draining twice.
pub async fn drain_for_exit(
    mut session: Box<dyn AgentSession>,
) -> yunta_core::Result<(Vec<AgentEvent>, Option<SessionExit>)> {
    let events = to_the_end(&mut session).await;
    let exit = session.exit().await?;
    Ok((events, exit))
}

async fn to_the_end(session: &mut Box<dyn AgentSession>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    let mut stream = session.events();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    events
}

/// Writes `lines` as a file under `dir`, newline-terminated — what a
/// scripted CLI prints, as the adapter's parser meets it. An empty
/// script is an empty file, never a lone newline.
pub fn write_lines(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let contents = if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    };
    std::fs::write(&path, contents).expect("the script writes");
    path
}

/// Waits until `pid` names no process at all.
///
/// A process a group signal reached is a zombie until whoever adopted it
/// reaps it, and a zombie still answers a signal, so "the group is gone"
/// is a question with a moment of latency in it. The deadline turns a
/// survivor into a failed test rather than a hung one.
pub async fn wait_until_gone(pid: Pid) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while liveness(pid) != Liveness::Dead {
        assert!(
            std::time::Instant::now() < deadline,
            "`{pid}` outlived the session that owned it"
        );
        tokio::task::yield_now().await;
    }
}

/// The fifo a scripted CLI records its child pid into. A fifo, not a
/// plain file, so [`grandchild_pid`] blocks on it and wakes the instant
/// the stub writes: a rendezvous with the child rather than a poll of
/// the filesystem.
pub fn child_pid_fifo(dir: &Path) -> PathBuf {
    let path = dir.join("child.pid");
    let status = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "mkfifo creates the child-pid fifo");
    path
}

/// The pid of the blocking child the stub spawned. The path is a fifo,
/// so the read blocks until the stub opens it and writes: an explicit
/// rendezvous, not a timed poll. The deadline turns a stub that never
/// records the pid into a failed test rather than a hung one.
pub async fn grandchild_pid(child_pid_fifo: &Path) -> String {
    let fifo = child_pid_fifo.to_path_buf();
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || std::fs::read_to_string(fifo)),
    )
    .await
    .expect("the stub records the child pid before the deadline")
    .expect("the pid reader joins")
    .expect("the child-pid fifo reads")
    .trim()
    .to_string()
}
