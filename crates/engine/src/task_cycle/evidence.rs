//! A file the task's own red output points at: evidence that its work
//! reaches there, which widens its scope with nobody asked.
//!
//! Two doors lead here. The fence refused a write and a red criterion of
//! the same attempt locates that file — `src/caller.rs:12`, as a compiler
//! or a test runner points at a place. Or the session asks for the paths
//! citing one of its task's red criteria as `evidence`, and that
//! criterion, run on its work, locates every one. Either way the grant
//! rests on what the engine verified, not on what anyone decided: it
//! never applies under `deny`, never reaches what no session may write,
//! counts toward `max_per_run`, and a path another task of the batch may
//! write is a person's to decide.

use std::path::Path;

use yunta_core::events::ToolTarget;
use yunta_core::{ScopeExpansionMode, ScopeGlob, Task};

use super::attempt::AttemptParams;
use super::{CriterionRun, TaskCycleError};
use crate::process::Printed;
use crate::scope_expansion::{Decision, ScopeExpansionOutcome, ScopeExpansionRequest};

/// The grant this attempt's refused writes earn, when a red criterion of
/// it locates them. `None` when nothing it refused is located, and under
/// `deny`.
pub(super) async fn of_refusals(
    params: &AttemptParams<'_>,
    refused: &[ToolTarget],
    post: &[CriterionRun],
) -> Option<ScopeExpansionOutcome> {
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
        let located: Vec<String> = refused
            .iter()
            .filter(|path| locates(&text, path))
            .cloned()
            .collect();
        if !located.is_empty() {
            evidence = Some((run.cmd.clone(), located));
            break;
        }
    }
    let (cmd, paths) = evidence?;
    let reason = format!(
        "the fence refused writing {} and `{cmd}` points there",
        paths.join(", ")
    );
    granted(params, paths, cmd, reason).await
}

/// The grant a request citing `evidence` earns: the criterion is one the
/// task declares, it is red on the session's work, and what it printed
/// locates every path asked for. `None` when any of that does not hold,
/// and the request is decided like any other.
pub(super) async fn of_request(
    params: &AttemptParams<'_>,
    request: &ScopeExpansionRequest,
) -> Result<Option<ScopeExpansionOutcome>, TaskCycleError> {
    match request.evidence.as_deref() {
        Some(cmd) if own(params.task, cmd) => located_by(params, request, cmd).await,
        _ => Ok(None),
    }
}

/// The grant a request the loop's rules did not grant earns from this
/// attempt's own check: a criterion it ran red — the task's own, or a
/// guard that holds it — whose output locates every path asked for. What
/// the engine can verify, nobody is asked. `None` when no red output does.
pub(super) async fn of_red(
    params: &AttemptParams<'_>,
    request: &ScopeExpansionRequest,
    post: &[CriterionRun],
) -> Option<ScopeExpansionOutcome> {
    let worktree = params.unit.worktree.as_path();
    let asked: Vec<String> = request
        .paths
        .iter()
        .map(|glob| glob.as_str().to_string())
        .collect();
    for run in post {
        let red = run.exit_code != 0 && run.could_not_run().is_none();
        let Some(printed) = run.output.as_ref().filter(|_| red) else {
            continue;
        };
        let text = text_of(printed, worktree, params).await;
        if !asked.is_empty() && asked.iter().all(|path| locates(&text, path)) {
            let reason = request.reason.clone();
            return granted(params, asked, run.cmd.clone(), reason).await;
        }
    }
    None
}

/// The grant `request` earns when `cmd`, run on the session's work, is red
/// and locates every path it asks for.
async fn located_by(
    params: &AttemptParams<'_>,
    request: &ScopeExpansionRequest,
    cmd: &str,
) -> Result<Option<ScopeExpansionOutcome>, TaskCycleError> {
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
    let asked: Vec<String> = request
        .paths
        .iter()
        .map(|glob| glob.as_str().to_string())
        .collect();
    if !red || !asked.iter().all(|path| locates(&text, path)) {
        return Ok(None);
    }
    Ok(granted(params, asked, cmd.to_string(), request.reason.clone()).await)
}

/// Whether `cmd` is one of the criteria `task` declares — never a command
/// the session made up to point somewhere.
fn own(task: &Task, cmd: &str) -> bool {
    task.criteria.iter().any(|criterion| criterion.cmd == cmd)
}

/// What the evidence for `paths` comes to under the loop's policy.
async fn granted(
    params: &AttemptParams<'_>,
    paths: Vec<String>,
    cmd: String,
    reason: String,
) -> Option<ScopeExpansionOutcome> {
    let mode = params
        .scope_expansion
        .map(|policy| policy.mode)
        .unwrap_or_default();
    let globs: Vec<ScopeGlob> = paths
        .iter()
        .filter_map(|path| ScopeGlob::exact(Path::new(path)).ok())
        .filter(|glob| !yunta_core::reaches_any(glob, params.denied))
        .collect();
    let provisional = decide(mode, &globs, params.beside)?;
    let cap = params.scope_expansion.and_then(|policy| policy.max_per_run);
    let decision = params.grants.commit(cap, provisional).await;
    Some(ScopeExpansionOutcome {
        request: ScopeExpansionRequest {
            paths: globs,
            reason,
            proposed_criterion: None,
            evidence: Some(cmd),
        },
        precheck_exit: None,
        decision,
    })
}

/// What evidence for `paths` decides under `mode`, before the cap: a
/// grant, or a person's decision when another task of the batch may
/// write one of them. Nothing under `deny`, or with nothing left to
/// grant.
fn decide(mode: ScopeExpansionMode, paths: &[ScopeGlob], beside: &[ScopeGlob]) -> Option<Decision> {
    if mode == ScopeExpansionMode::Deny || paths.is_empty() {
        return None;
    }
    match paths
        .iter()
        .any(|path| yunta_core::reaches_any(path, beside))
    {
        true => Some(Decision::Escalate),
        false => Some(Decision::Granted),
    }
}

/// Whether `printed` locates a write of `refused`, read the way the
/// attempt's close reads it: relative to `worktree`.
pub(crate) fn locates_a_refusal(printed: &str, worktree: &Path, refused: &[ToolTarget]) -> bool {
    let text = relative_text(printed.to_string(), worktree);
    refused
        .iter()
        .filter_map(|target| relative(worktree, target.display()?))
        .any(|path| locates(&text, &path))
}

/// `target` relative to the checkout, when it lies in it.
fn relative(worktree: &Path, target: &str) -> Option<String> {
    let path = Path::new(target);
    if path.is_relative() {
        return Some(target.to_string());
    }
    roots(worktree)
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .map(|inside| inside.to_string_lossy().into_owned())
}

/// The checkout's path as a session or a command may spell it: as the
/// run named it, and as the filesystem resolves it.
fn roots(worktree: &Path) -> Vec<std::path::PathBuf> {
    let mut roots = vec![worktree.to_path_buf()];
    roots.extend(worktree.canonicalize().ok().filter(|real| real != worktree));
    roots
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

/// `text` with the checkout's own path taken out, so a location reads
/// relative to it whichever way a command printed it.
fn relative_text(text: String, worktree: &Path) -> String {
    roots(worktree).iter().fold(text, |text, root| {
        text.replace(&format!("{}/", root.display()), "")
    })
}

/// Whether `text` locates `path`: names it whole — not as the tail of
/// another path, not by its stem — with a line number right after it, as
/// `path:12`.
pub(crate) fn locates(text: &str, path: &str) -> bool {
    text.match_indices(path).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = &text[at + path.len()..];
        let whole = before.is_none_or(|c| !(c.is_alphanumeric() || "_./-".contains(c)));
        let line = after
            .strip_prefix(':')
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
        whole && line
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALLER: &str = "crates/testkit/src/bench/driving.rs";

    #[test]
    fn a_location_names_the_whole_path_and_a_line() {
        assert!(locates(
            "error[E0061]\n  --> crates/testkit/src/bench/driving.rs:378:9",
            CALLER
        ));
        assert!(locates(
            "at (crates/testkit/src/bench/driving.rs:12)",
            CALLER
        ));
    }

    #[test]
    fn stem_only_mention_is_not_evidence() {
        assert!(!locates("driving.rs:378", CALLER));
        assert!(!locates(
            "see crates/testkit/src/bench/driving.rs for details",
            CALLER
        ));
        assert!(!locates(
            "other/crates/testkit/src/bench/driving.rs:3",
            CALLER
        ));
        assert!(!locates("driving:3 failed", CALLER));
    }

    #[test]
    fn deny_mode_grants_nothing() {
        let paths = [ScopeGlob::from("b.rs")];
        assert_eq!(decide(ScopeExpansionMode::Deny, &paths, &[]), None);
        assert_eq!(
            decide(ScopeExpansionMode::Ask, &paths, &[]),
            Some(Decision::Granted)
        );
    }

    #[test]
    fn path_of_a_concurrent_task_is_escalated() {
        let paths = [ScopeGlob::from("i/b.rs")];
        let beside = [ScopeGlob::from("i/**")];
        let decided = decide(ScopeExpansionMode::Rules, &paths, &beside);
        assert_eq!(decided, Some(Decision::Escalate));
    }
}
