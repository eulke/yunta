//! Starting a governed command: its leader in a process group of its own,
//! registered for its run, and the tasks that carry its stdin and read its
//! collected streams.

use std::process::Stdio;

use tokio::task::JoinSet;
use yunta_core::Pid;

use super::pipes::{read_to_capture, stdio, write_then_close, Captured};
use super::{GovernedCommand, PipeKind, Pipes, SpawnError, Supervision};
use crate::process_registry;

/// Starts `command` as the leader of its own process group, with the run's
/// variables layered onto the inherited environment, and returns it with the
/// group's id.
pub(super) fn spawn_leader(
    command: &GovernedCommand,
    supervision: Supervision<'_>,
    described: &str,
) -> Result<(tokio::process::Child, Pid), SpawnError> {
    let mut std_cmd = std::process::Command::new(&command.program);
    std_cmd
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(supervision.env.iter().map(|(k, v)| (k, v)))
        .stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(stdio(command.stdout))
        .stderr(stdio(command.stderr));
    // The child leads its own group, so a kill reaches every process
    // it spawned in turn, not just the one the engine started.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        std_cmd.process_group(0);
    }
    let child = tokio::process::Command::from(std_cmd)
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| SpawnError::Spawn {
            command: described.to_string(),
            source,
        })?;
    let pgid = process_registry::child_pid(&child).ok_or_else(|| SpawnError::NoPid {
        command: described.to_string(),
    })?;
    Ok((child, pgid))
}

/// Takes the child's pipes and starts a task for each: the stdin write when
/// there are bytes to give, and the read of each collected stream.
pub(super) fn start_pipes(
    child: &mut tokio::process::Child,
    stdin: Option<Vec<u8>>,
    stdout: &Captured,
    stderr: &Captured,
) -> Pipes {
    let mut pipes = JoinSet::new();
    if let Some((bytes, pipe)) = stdin.zip(child.stdin.take()) {
        pipes.spawn(async move { (PipeKind::Stdin, write_then_close(pipe, &bytes).await) });
    }
    if let Some(pipe) = child.stdout.take() {
        let capture = stdout.clone();
        pipes.spawn(async move { (PipeKind::Stdout, read_to_capture(pipe, capture).await) });
    }
    if let Some(pipe) = child.stderr.take() {
        let capture = stderr.clone();
        pipes.spawn(async move { (PipeKind::Stderr, read_to_capture(pipe, capture).await) });
    }
    pipes
}
