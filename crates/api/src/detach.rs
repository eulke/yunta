//! See [`crate`]. Handing a run to a detached Yunta: the run created here and
//! started by an independent `yunta resume`, or a parked run handed on to one,
//! without waiting for it. No new execution path: the detached child is an
//! ordinary resume, exactly what a person would run by hand, started from the
//! workspace's own binary in the workspace's directory.
//!
//! The child's log goes to `run.dir/scratch/detached.log`, never silently
//! discarded; its process group is its own, so a signal to the caller's group
//! (a shell's Ctrl-C) never reaches it. The caller still owns reaping: a task
//! awaits the child's exit, so a long-lived server that starts runs collects
//! every child it started.

use std::path::Path;

use chrono::{DateTime, Utc};
use yunta_core::events::PauseReason;
use yunta_core::step::NextStep;
use yunta_core::{Isolation, Pid, RunId};
use yunta_engine::process::Supervision;
use yunta_render::command::{self, Reader};
use yunta_replay::RunState;

use crate::{CreatedRun, Error, Failure, OpenedRun, Stop, Workspace};

impl CreatedRun<'_> {
    /// Starts the created run in a detached `yunta resume` and hands back its
    /// id without waiting for it. The checkout this process claimed while it
    /// created the run is handed to the child; when that fails the child is
    /// ended before the claim is released, so nothing works in the tree while
    /// the claim names nobody.
    pub async fn start_detached(self, stop: &Stop) -> Result<RunId, Error> {
        let workspace = &self.project.workspace;
        let isolation = self.manifest.isolation;
        match resume_detached(workspace, &self.id, &self.dir).await {
            Ok(child) => {
                hand_off(child, isolation, &workspace.dir, workspace.teardown(stop)).await?;
                Ok(self.id)
            }
            Err(failed) => {
                let teardown = workspace.teardown(stop);
                yunta_engine::release_worktree(&workspace.dir, isolation, teardown).await?;
                Err(failed)
            }
        }
    }
}

impl OpenedRun<'_> {
    /// Hands the run on to a detached `yunta resume`, which drives it forward
    /// independently, and hands back the child's pid without waiting for it.
    pub async fn resume_detached(&self) -> Result<Pid, Error> {
        resume_detached(&self.project.workspace, &self.id, &self.dir).await
    }
}

/// A detached `yunta resume` that never started, and the run it was for.
/// Recover by running that command yourself: the run is on disk and
/// unchanged, so nothing is lost by handing it forward by hand. Every
/// interface that hands a run off reports the same failure, with the run's
/// whole id, since the control plane is one of them.
#[derive(Debug, thiserror::Error)]
#[error(
    "cannot spawn a detached `{}`: {source}",
    command::line_for(&NextStep::Resume { run: RunId::clone(.run) }, Reader::Agent)
)]
struct DetachedResumeError {
    run: RunId,
    #[source]
    source: std::io::Error,
}

/// Starts a detached `yunta resume` of run `id`, whose directory is `dir`,
/// from `workspace`'s binary in its directory, and hands back the child's
/// pid: who a caller holding a claim on what the child is about to work in
/// moves it to.
pub(crate) async fn resume_detached(
    workspace: &Workspace,
    id: &RunId,
    dir: &Path,
) -> Result<Pid, Error> {
    let failed = |source| -> Error {
        Failure::new(DetachedResumeError {
            run: id.clone(),
            source,
        })
        .into()
    };
    let log_path = dir
        .join(yunta_engine::run_dir::SCRATCH_DIR)
        .join("detached.log");
    let log = detached_log(&log_path).await.map_err(failed)?;
    let log_err = log.try_clone().map_err(failed)?;
    let mut child = tokio::process::Command::new(&workspace.binary);
    child
        .arg("resume")
        .arg(id.as_str())
        .current_dir(&workspace.dir)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(log_err);
    #[cfg(unix)]
    child.process_group(0);
    let mut child = child.spawn().map_err(failed)?;
    let pid = child
        .id()
        .and_then(|id| Pid::try_from(id).ok())
        .ok_or_else(|| {
            failed(std::io::Error::other(
                "the detached child reported no usable process id",
            ))
        })?;
    tokio::spawn(async move {
        // best-effort: the reaper only collects the child; its own log says how the run went.
        let _ = child.wait().await;
    });
    Ok(pid)
}

/// The detached log at `path`, opened to add to it: a run that pauses on a
/// reset again hands itself to a later link of the same chain, and each
/// link's log follows the one before it instead of erasing it.
async fn detached_log(path: &Path) -> std::io::Result<std::fs::File> {
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    Ok(file.into_std().await)
}

/// Hands the claim on this checkout to `child`. When the hand-over fails, the
/// child is ended before the claim is released.
async fn hand_off(
    child: Pid,
    isolation: Isolation,
    dir: &Path,
    teardown: Supervision<'_>,
) -> Result<(), Error> {
    let Err(error) = yunta_engine::hand_over_worktree(dir, isolation, child, teardown).await else {
        return Ok(());
    };
    if let Err(kill) = yunta_core::process::group::force_kill_group(child).await {
        tracing::warn!(%kill, "the detached child outlived a failed hand-over");
    }
    if let Err(release) = yunta_engine::release_worktree(dir, isolation, teardown).await {
        tracing::warn!(%release, "the claim could not be released after a failed hand-over");
    }
    Err(error.into())
}

/// The instant a run paused on a limit's reset resumes at, or `None` for any
/// other run. A resume of such a run waits for that instant before it takes
/// anything.
pub(crate) fn reset_at(state: &RunState) -> Option<DateTime<Utc>> {
    match state.run.paused() {
        Some((PauseReason::AwaitingReset { at, .. }, _)) => Some(*at),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::CommandExt;

    use tokio_util::sync::CancellationToken;
    use yunta_testkit_core::FixedClock;

    use super::*;

    #[tokio::test]
    async fn a_failed_hand_off_ends_the_child_before_the_claim_is_released() {
        // No common git directory here, so the hand-over cannot write its lock.
        let not_a_repo = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = Pid::try_from(child.id()).unwrap();
        let cancel = CancellationToken::new();
        let refused = hand_off(
            pid,
            Isolation::None,
            not_a_repo.path(),
            Supervision::outside_any_run(&cancel, &FixedClock),
        )
        .await;
        assert!(refused.is_err());
        yunta_testkit::wait_until_async(
            || std::future::ready(matches!(child.try_wait(), Ok(Some(_)))),
            || "the child outlived the failed hand-over".into(),
        )
        .await;
    }

    #[tokio::test]
    async fn a_later_link_of_a_detached_chain_adds_to_the_log() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("detached.log");
        for line in ["first link\n", "second link\n"] {
            detached_log(&path)
                .await
                .unwrap()
                .write_all(line.as_bytes())
                .unwrap();
        }
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "first link\nsecond link\n");
    }
}
