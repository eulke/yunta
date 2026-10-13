//! The facts a red output of one attempt locates: a path the output names
//! as `path:line`, and the criterion that printed it. A refused write, or a
//! request citing one of the task's own criteria, is located this way. The
//! decision on those facts is `yunta_core::expansion`'s, reached through
//! `scope_expansion::expand`.
//!
//! Two doors lead here. The fence refused a write and a red criterion of the
//! same attempt locates that file — `src/caller.rs:12`, as a compiler or a
//! test runner points at a place. Or the session asks for the paths citing
//! one of its task's criteria as `evidence`, and that criterion, run on its
//! work, locates every one. Either way the facts are what the engine
//! verified, and nobody is asked to confirm them.

use std::path::Path;

use yunta_core::events::ToolTarget;
use yunta_core::expansion::Precheck;
use yunta_core::{ScopeGlob, Task};

use super::attempt::AttemptParams;
use super::{CriterionRun, TaskCycleError};
use crate::process::Printed;
use crate::scope_expansion::{
    locates, relative, relative_text, ScopeExpansionOutcome, ScopeExpansionRequest,
};

/// What a red output locates, owned so that a decision can borrow it: the
/// paths, the criterion that located them, the request the session made
/// (none for a refused write) and the pre-check of that request's criterion.
pub(super) struct Located {
    pub(super) paths: Vec<ScopeGlob>,
    pub(super) criterion: String,
    pub(super) asked: Option<ScopeExpansionRequest>,
    pub(super) precheck: Option<Precheck>,
}

/// What a located decision is made on: at least one path. Nothing located
/// is nothing to decide.
fn located(
    paths: Vec<ScopeGlob>,
    criterion: String,
    asked: Option<ScopeExpansionRequest>,
    precheck: Option<Precheck>,
) -> Option<Located> {
    (!paths.is_empty()).then_some(Located {
        paths,
        criterion,
        asked,
        precheck,
    })
}

/// The first red output of this attempt that names one of the paths its
/// fence refused, with that path. `None` when no red output names one.
pub(super) async fn of_refusals(
    params: &AttemptParams<'_>,
    refused: &[ToolTarget],
    post: &[CriterionRun],
) -> Option<Located> {
    let worktree = params.unit.worktree.as_path();
    let refused: Vec<String> = refused
        .iter()
        .filter_map(|target| relative(worktree, target.display()?))
        .collect();
    let mut evidence = None;
    for run in post {
        let red = run.exit_code != 0 && run.could_not_run().is_none();
        let Some(printed) = run.output.as_ref().filter(|_| red) else {
            continue;
        };
        let text = text_of(printed, worktree, params).await;
        let here: Vec<String> = refused
            .iter()
            .filter(|path| locates(&text, path))
            .cloned()
            .collect();
        if !here.is_empty() {
            evidence = Some((run.cmd.clone(), here));
            break;
        }
    }
    let (criterion, paths) = evidence?;
    let globs = paths
        .iter()
        .filter_map(|path| ScopeGlob::exact(Path::new(path)).ok())
        .collect();
    located(globs, criterion, None, None)
}

/// The facts of the request, when it cites one of the task's criteria as
/// its evidence and that criterion locates every path it asks for. `None`
/// when any of that does not hold, and the request is decided on its own.
pub(super) async fn of_request(
    params: &AttemptParams<'_>,
    request: &ScopeExpansionRequest,
) -> Result<Option<Located>, TaskCycleError> {
    match request.evidence.as_deref() {
        Some(cmd) if own(params.task, cmd) => located_by(params, request, cmd).await,
        _ => Ok(None),
    }
}

/// The facts of a request the decision on its own left open: a red criterion
/// of this attempt that locates every path it asks for, which keeps the
/// request's own pre-check. `None` when no red output locates them all.
pub(super) async fn of_red(
    params: &AttemptParams<'_>,
    outcome: &ScopeExpansionOutcome,
    post: &[CriterionRun],
) -> Option<Located> {
    let request = outcome.asked.as_ref()?;
    let worktree = params.unit.worktree.as_path();
    for run in post {
        let red = run.exit_code != 0 && run.could_not_run().is_none();
        let Some(printed) = run.output.as_ref().filter(|_| red) else {
            continue;
        };
        let text = text_of(printed, worktree, params).await;
        if request
            .paths
            .iter()
            .all(|path| locates(&text, path.as_str()))
        {
            return located(
                request.paths.clone(),
                run.cmd.clone(),
                Some(request.clone()),
                outcome.precheck,
            );
        }
    }
    None
}

/// Whether `cmd` is one of the criteria `task` declares — never a command
/// the session made up to point somewhere.
fn own(task: &Task, cmd: &str) -> bool {
    task.criteria.iter().any(|criterion| criterion.cmd == cmd)
}

/// The facts of `request` when `cmd`, run on the session's work, is red and
/// its output locates every path the request asks for.
async fn located_by(
    params: &AttemptParams<'_>,
    request: &ScopeExpansionRequest,
    cmd: &str,
) -> Result<Option<Located>, TaskCycleError> {
    let worktree = params.unit.worktree.as_path();
    let answer = params
        .memo
        .exit_code(cmd, worktree, params.supervision)
        .await?;
    let red = answer.exit_code != 0 && super::criteria::could_not_run(answer.exit_code).is_none();
    let Some(printed) = answer.output.as_ref() else {
        return Ok(None);
    };
    let text = text_of(printed, worktree, params).await;
    if !red
        || !request
            .paths
            .iter()
            .all(|path| locates(&text, path.as_str()))
    {
        return Ok(None);
    }
    Ok(located(
        request.paths.clone(),
        cmd.to_string(),
        Some(request.clone()),
        None,
    ))
}

/// Everything `printed` says, with the checkout's own path taken out, so
/// a location reads relative to it whichever way the command printed it.
/// An answer an earlier invocation gave is read from the object the run
/// kept of it; only one whose object cannot be read — or that a log
/// written before a run kept its objects names none for — is read from
/// the last lines the log quotes.
async fn text_of(printed: &Printed, worktree: &Path, params: &AttemptParams<'_>) -> String {
    let text = match printed {
        Printed::Ran(output) => String::from_utf8_lossy(output.bytes()).into_owned(),
        Printed::Recorded { object, tail } => {
            let kept = match (object, params.audit) {
                (Some(hash), Some((observer, _))) => observer.kept_output(hash).await,
                _ => None,
            };
            match kept {
                Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                None => tail.join("\n"),
            }
        }
    };
    relative_text(text, worktree)
}
