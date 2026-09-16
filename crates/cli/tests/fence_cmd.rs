//! `yunta fence <adapter-id>` end to end: the real binary, a real call
//! on stdin, and the exit code the calling CLI reads.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use yunta_core::fence::{Advice, Fence};

/// A `PreToolUse` call for the `claude-code` codec, writing `path`.
fn writing(path: &str) -> String {
    format!(r#"{{"tool_name":"Write","tool_input":{{"file_path":"{path}"}}}}"#)
}

/// Runs the hook with `fence` in its environment and the call on stdin.
fn hook(worktree: &Path, fence: Option<&Fence>, stdin: &str) -> std::process::Output {
    let away = tempfile::tempdir().expect("a directory for this test's home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_yunta"));
    yunta_testkit::hermetic(&mut command, away.path(), &away.path().join("state"));
    command
        .args(["fence", "claude-code"])
        .env_remove(yunta_core::fence::ENV_VAR)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(fence) = fence {
        let (var, value) = fence.to_env(worktree);
        command.env(var, value);
    }
    let mut child = command.spawn().expect("the hook runs");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("the call reaches the hook");
    child.wait_with_output().expect("the hook answers")
}

#[test]
fn the_fence_command_refuses_with_exit_two_and_the_reason_on_stderr() {
    let worktree = Path::new("/work/task-1");
    let fence = Fence {
        allowed: Some(vec!["src/**".into()]),
        roots: Vec::new(),
        advice: Advice::RequestExpansion,
    };

    let out = hook(
        worktree,
        Some(&fence),
        &writing("/work/task-1/docs/readme.md"),
    );

    assert_eq!(out.status.code(), Some(2), "a refusal is exit 2");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains(yunta_core::fence::REFUSAL_MARKER) && said.contains("docs/readme.md"),
        "the model reads why, and what it tried: {said}"
    );
}

#[test]
fn the_fence_command_allows_with_exit_zero_and_nothing_on_stdout() {
    let worktree = Path::new("/work/task-1");
    let fence = Fence {
        allowed: Some(vec!["src/**".into()]),
        roots: Vec::new(),
        advice: Advice::RequestExpansion,
    };

    let out = hook(worktree, Some(&fence), &writing("/work/task-1/src/lib.rs"));

    assert_eq!(out.status.code(), Some(0), "consent is exit 0");
    assert!(
        out.stdout.is_empty(),
        "consent says nothing: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// A failure of ours refuses; it never allows. A hook with no fence in
/// its environment has no idea what the session may write.
#[test]
fn a_misconfigured_fence_refuses_rather_than_allows() {
    let out = hook(
        Path::new("/work/task-1"),
        None,
        &writing("/work/task-1/src/lib.rs"),
    );

    assert_eq!(out.status.code(), Some(2), "a misconfiguration refuses");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("fence misconfigured"),
        "the reason names itself as ours: {said}"
    );
}
