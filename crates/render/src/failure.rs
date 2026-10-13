//! The words a failure is told in: its headline, what it says beyond the
//! headline, and the sentence a log and `--json` read it as. Every surface
//! that says why a node stopped calls these, so none of them can say a
//! different thing. The words of a command, a session and a limit are
//! written here beside the failure they belong to.

use std::path::PathBuf;

use yunta_core::events::{
    AccountLimit, CommandExit, CommandOrigin, Fact, Failure, LimitSource, OwedScope, RedCheck,
    RefusalCause, SessionDeath, SessionEnd, SessionRefusal, TaskStop, UnpassableTask,
};
use yunta_core::protected::Protection;
use yunta_core::{listed_globs, text, RunId};

/// How a limit's reset moment reads: the day and the minute, in UTC, as a
/// person reads a log's clock.
pub(crate) const RESET_FORMAT: &str = "%Y-%m-%d %H:%M UTC";

/// The claim a failure is told by: what a surface states before it lists
/// what the failure says beyond it. One line, except for a loop, whose
/// claim is the list of tasks that stopped it.
pub fn headline(failure: &Failure) -> String {
    match failure {
        Failure::Exited { exited } => command_headline(exited),
        Failure::Unchanged { unchanged } => {
            unchanged_sentence(unchanged.since, &headline(&unchanged.failure))
        }
        // Each document that did not close, by the heading its own block
        // opens with: the path and how many problems it has.
        Failure::Artifacts { artifacts } => artifacts
            .iter()
            .filter_map(|artifact| artifact.to_string().lines().next().map(str::to_string))
            .collect::<Vec<_>>()
            .join("; "),
        // How many files, when there are several to list.
        Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => format!(
            "scope violated: {} outside the declared globs",
            text::counted(outside_scope.len(), "file")
        ),
        Failure::Red { red } => red_headline(red),
        Failure::ChildFailed {
            child_run,
            child_failure: Some(child),
        } => format!("child run `{child_run}` failed: {}", headline(child)),
        _ => sentence(failure),
    }
}

/// What the failure says beyond its [`headline`], a line each: the last
/// lines a command printed, the problems of a document that did not close
/// (each document's own block when several did), every path that fell
/// outside a scope when there are several, and the lines of a failed
/// child run. Nothing else: the headline says the rest.
pub fn detail(failure: &Failure) -> Vec<String> {
    match failure {
        Failure::Exited { exited } => exited.tail.clone(),
        Failure::Unchanged { unchanged } => detail(&unchanged.failure),
        Failure::Artifacts { artifacts } => {
            let blocks = artifacts.iter().map(ToString::to_string);
            match artifacts.len() {
                1 => blocks.flat_map(|block| own_lines(&block, 1)).collect(),
                _ => blocks.flat_map(|block| own_lines(&block, 0)).collect(),
            }
        }
        Failure::ScopeViolated { outside_scope } if outside_scope.len() > 1 => outside_scope
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        Failure::ScopeOwed { owed } => owed.iter().map(owed_line).collect(),
        Failure::Red {
            red:
                RedCheck::Command {
                    conclusion: Some(said),
                    ..
                },
        } => said.lines().map(str::to_string).collect(),
        Failure::Red {
            red: RedCheck::Regression { tail, .. },
        } => tail.clone(),
        Failure::ChildFailed {
            child_failure: Some(child),
            ..
        } => detail(child),
        _ => Vec::new(),
    }
}

/// The failure as one sentence, the way a log and `--json` read it: every
/// line a command printed included, and none of it left out.
pub fn sentence(failure: &Failure) -> String {
    match failure {
        Failure::Message { outcome } => outcome.clone(),
        Failure::Exited { exited } => command_exit(exited),
        Failure::Unset { unset } => unset.to_string(),
        Failure::Unchanged { unchanged } => {
            unchanged_sentence(unchanged.since, &sentence(&unchanged.failure))
        }
        Failure::SessionDied { died } => session_death(died),
        Failure::ScopeViolated { outside_scope } => format!(
            "scope violated: {} outside the declared globs — {}",
            text::counted(outside_scope.len(), "file"),
            paths_listed(outside_scope)
        ),
        Failure::PathsDenied {
            denied_paths,
            reasons,
        } => denied_sentence(denied_paths, reasons),
        Failure::ScopeRequested { requested_scope } => format!(
            "asked for scope beyond its own — {}: {}",
            listed_globs(&requested_scope.paths),
            requested_scope.reason
        ),
        Failure::ScopeOwed { owed } => owed_sentence(owed),
        Failure::PlanUnpassable { unpassable } => plan_sentence(unpassable),
        Failure::Artifacts { artifacts } => artifacts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        Failure::SessionRefused { session_refused } => refusal_sentence(session_refused),
        Failure::Limit { limit } => limit_sentence(limit),
        Failure::TasksStopped { stopped } => tasks_stopped_sentence(stopped),
        Failure::Red { red } => red_sentence(red),
        Failure::ChildFailed {
            child_run,
            child_failure,
        } => child_sentence(child_run, child_failure.as_deref()),
    }
}

/// Every fact a failure is recorded with beside its menu: its headline,
/// then each line of its detail, each a fact of its own.
pub fn facts(failure: &Failure) -> Vec<Fact> {
    std::iter::once(Fact::bare(headline(failure)))
        .chain(detail(failure).into_iter().map(Fact::bare))
        .collect()
}

/// The line that names a command and how it ended, without what it printed.
pub fn command_headline(exit: &CommandExit) -> String {
    match &exit.origin {
        None => format!("exit {}", exit.code),
        Some(CommandOrigin::Executor { executor }) => {
            format!("executor `{executor}` exited {}", exit.code)
        }
        Some(CommandOrigin::Hook { phase, command }) => {
            format!("{} hook `{command}` failed", phase.as_str())
        }
    }
}

/// The headline and, under it, the lines the command printed last. A
/// command that failed saying nothing — `test -f x` is the ordinary case —
/// reads as its headline alone.
pub fn command_exit(exit: &CommandExit) -> String {
    text::detailed(command_headline(exit), &exit.tail.join("\n"))
}

/// A session that ended without a terminal event: whose it was, and how
/// its process went when it had one of its own.
pub fn session_death(death: &SessionDeath) -> String {
    let Some(exit) = &death.exit else {
        return format!("session `{}` ended without a terminal event", death.adapter);
    };
    let said = format!(
        "session `{}` {} before any terminal event",
        death.adapter,
        session_end(&exit.end)
    );
    match exit.stderr_tail.last() {
        Some(last) => format!("{said} — {last}"),
        None => said,
    }
}

/// How a session's process ended: the code it exited with, or the signal
/// that killed it.
pub fn session_end(end: &SessionEnd) -> String {
    match end {
        SessionEnd::Code { code } => format!("exited with code {code}"),
        SessionEnd::Signal { signal } => format!("was killed by signal {signal}"),
        SessionEnd::Unknown => "ended in a way this build does not know".to_string(),
    }
}

/// A session's refusal: the adapter that refused, what the refusal was
/// for, and the message it gave.
pub(crate) fn refusal_sentence(refusal: &SessionRefusal) -> String {
    let said = match &refusal.cause {
        RefusalCause::Final => format!("session `{}` refused the request", refusal.adapter),
        RefusalCause::Authentication => format!(
            "session `{}` refused the request (its credential was refused)",
            refusal.adapter
        ),
        RefusalCause::InvalidModel => format!(
            "session `{}` refused the request (the model is not one the account can use)",
            refusal.adapter
        ),
        RefusalCause::Unreachable => format!(
            "session `{}` refused the request (the service could not be reached)",
            refusal.adapter
        ),
    };
    text::detailed(said, &refusal.message)
}

/// An account's limit: who hit it, until when when the source named a
/// moment, and the message the source gave.
pub(crate) fn limit_sentence(limit: &AccountLimit) -> String {
    let who = match &limit.source {
        LimitSource::Session { adapter } => format!("session `{adapter}` hit a limit"),
        LimitSource::Forge => "the forge rate-limited its requests".to_string(),
    };
    let said = match &limit.resets_at {
        Some(at) => format!("{who} until {}", at.format(RESET_FORMAT)),
        None => who,
    };
    text::detailed(said, &limit.message)
}

/// The unchanged attempt's sentence: why it did not run, ending in what
/// the attempt that last ran failed with, as `failed` says it.
pub(crate) fn unchanged_sentence(since: u32, failed: &str) -> String {
    format!(
        "not run again: nothing in the run's tree changed since attempt {since} failed on \
         it — change what it failed on there first. Attempt {since} failed: {failed}"
    )
}

/// The words a protected path's reason reads as, after the path it names.
pub(crate) fn protection_words(protection: Protection) -> &'static str {
    match protection {
        Protection::ProjectDeny => "denied by the project's permissions",
        Protection::AgentConfig => "an agent CLI's own configuration",
        Protection::AgentInstructions => {
            "an instruction file: only a person grants it, and only to a session that asks"
        }
        Protection::ApprovedTest => "an approved test",
        Protection::GitRegister => "the run's git record",
    }
}

/// Paths as a sentence lists them: comma-separated, each as its display.
pub(crate) fn paths_listed(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The sentence of a red check: its headline, and under it what the check
/// printed last: the line a command concluded with, or the tail of a suite
/// that regressed.
fn red_sentence(red: &RedCheck) -> String {
    match red {
        RedCheck::Command { conclusion, .. } => {
            text::detailed(red_headline(red), conclusion.as_deref().unwrap_or_default())
        }
        RedCheck::Regression { tail, .. } => text::detailed(red_headline(red), &tail.join("\n")),
        RedCheck::Coverage { .. } => red_headline(red),
    }
}

fn red_headline(red: &RedCheck) -> String {
    match red {
        RedCheck::Coverage {
            measured: Some(measured),
            threshold,
            ..
        } => format!("coverage {measured}% is below the {threshold}% threshold"),
        RedCheck::Coverage {
            cmd,
            measured: None,
            ..
        } => format!("`{cmd}` produced no `NN[.NN]%` coverage figure in its stdout"),
        RedCheck::Command { cmd, exit_code, .. } => format!("`{cmd}` exits {exit_code}"),
        RedCheck::Regression { cmd, exit_code, .. } => {
            format!("regression: `{cmd}` passed at baseline (exit 0) but now exits {exit_code}")
        }
    }
}

/// The loop's words: the line that says why no task of it is ready, then
/// each stopped task and what stopped it.
fn tasks_stopped_sentence(stopped: &[TaskStop]) -> String {
    let mut said =
        "no task is ready and not all are done — blocked or failed tasks need a decision"
            .to_string();
    for stop in stopped {
        said.push_str("; ");
        said.push_str(&text::detailed(
            format!("task `{}` blocked", stop.task_id),
            &crate::blocks::stop::sentence(&stop.cause),
        ));
    }
    said
}

/// The paths a denial names, each with the reason it is protected when the
/// log says one: the reasons line up with the paths by index.
fn denied_sentence(paths: &[PathBuf], reasons: &[Protection]) -> String {
    let listed: Vec<String> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| match reasons.get(index) {
            Some(reason) => format!("{} ({})", path.display(), protection_words(*reason)),
            None => path.display().to_string(),
        })
        .collect();
    format!(
        "wrote what no session of the run may write — {}: undo those changes",
        listed.join(", ")
    )
}

/// The plan's unpassable criteria: each task, its criterion and why it cannot pass.
fn plan_sentence(unpassable: &[UnpassableTask]) -> String {
    format!(
        "the plan holds a criterion no work can pass — {}",
        unpassable
            .iter()
            .map(|task| format!("task `{}`: `{}` ({})", task.task_id, task.cmd, task.why))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

/// A child run that failed, named by its run, with what failed it under it
/// when the child's failure is known.
fn child_sentence(child_run: &RunId, child: Option<&Failure>) -> String {
    match child {
        Some(child) => format!(
            "child run `{child_run}` failed:\n  {}",
            text::hanging(&sentence(child), "  ")
        ),
        None => format!("child run `{child_run}` failed"),
    }
}

/// The requests a loop owes a person, as one sentence.
fn owed_sentence(owed: &[OwedScope]) -> String {
    let each: Vec<String> = owed
        .iter()
        .map(|owed| {
            format!(
                "task `{}` asked for {}",
                owed.task_id,
                listed_globs(&owed.paths)
            )
        })
        .collect();
    format!(
        "{} a person's decision about scope: {}",
        text::counted_as(owed.len(), "request owes", "requests owe"),
        each.join("; ")
    )
}

/// One request a loop owes a person: the task's paths, and its reason.
fn owed_line(owed: &OwedScope) -> String {
    format!(
        "`{}` — {}: {}",
        owed.task_id,
        listed_globs(&owed.paths),
        owed.reason
    )
}

/// The lines of `block` after its first `skip`, without their indent.
fn own_lines(block: &str, skip: usize) -> Vec<String> {
    block
        .lines()
        .skip(skip)
        .map(|line| line.trim().to_string())
        .collect()
}
