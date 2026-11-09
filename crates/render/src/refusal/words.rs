//! See [`super`]. The sentences for what Yunta refuses about a run, a
//! command's inputs and a pack, each in the vocabulary of the command that
//! meets it.

use std::path::Path;

use crate::command;
use surco::words::Standing;
use yunta_core::check::CheckError;
use yunta_core::events::{Claim, FailureMenu, Refusal};
use yunta_core::inputs::InputsProblem;
use yunta_core::refusal::Unservable;
use yunta_core::refusal::{AnswersRefusal, CloseRefusal, GateRefusal};
use yunta_core::step::{Arg, NextStep, Reach};
use yunta_core::{AdapterId, PackRef, Pid};
use yunta_replay::receipt::ReceiptError;

/// What `last` says where the repository it is asked in has no run, and the
/// command that lists every run instead.
pub(super) fn no_run_here() -> String {
    format!(
        "no run in this repository; `{}` lists every run",
        command::line(&NextStep::ListRuns {
            reach: Reach::Machine
        })
    )
}

/// What `needs` says where no run of the repository it is asked in waits on a
/// person, and the command that lists them.
pub(super) fn none_needs_you() -> String {
    format!(
        "no run of this repository {}; `{}` lists them",
        Standing::NeedsYou.word(),
        command::line(&NextStep::ListRuns {
            reach: Reach::Repository
        })
    )
}

/// What a receipt asked of a run that has not finished says: the state the
/// run is in, then the command that shows where it stands, which is this
/// border's vocabulary rather than the derivation's.
pub(super) fn no_receipt(error: &ReceiptError) -> String {
    let ReceiptError::NotFinished(run) = error;
    format!(
        "{error}; `{}` shows where it is",
        command::line(&NextStep::status(run))
    )
}

/// Why git cannot name who commits in a repository, and what sets it: the
/// last line of what git said is the reason it gave.
pub fn unnamed_committer(git_said: &str) -> String {
    format!(
        "cannot name who commits here: set `git config --global user.name \"Your Name\"` \
         and `git config --global user.email you@example.com` (without `--global`, for \
         this repository alone); git said: {}",
        git_said.lines().last().unwrap_or_default()
    )
}

/// What a run that would open a pull request through a forge it cannot reach
/// says: the repository, and the variable its token is in.
pub(super) fn forge_unreachable(repo: &str, token_env: &str) -> String {
    format!(
        "this workflow opens a pull request through `forge.github` ({repo}), and `{token_env}`, \
         the variable its token is in, is not set here; set it, then run again"
    )
}

/// What a run past the project's cap on concurrent runs says: how many are
/// still going, and the command that names them.
pub(super) fn too_many_runs(active: usize, cap: u32) -> String {
    format!(
        "{} still active and `limits.max_concurrent_runs` is {cap}; resume or cancel one \
         (`{list}` names them) before starting another",
        surco::text::counted(active, "run"),
        list = command::line(&NextStep::List)
    )
}

/// What a run that took over a dead process's lock on this checkout says:
/// whose it was.
pub fn stale_lock_taken(pid: Pid) -> String {
    format!("this checkout's isolation lock belonged to a dead process (pid {pid}): taking it over")
}

/// What this binary cannot serve for a run, naming what a reader changes.
pub(super) fn unservable(unservable: &Unservable) -> String {
    match unservable {
        Unservable::NoCandidateOnPin {
            node,
            runner,
            pin,
            candidates,
        } => format!(
            "node `{node}` runs on runner `{runner}`, which has no candidate on `--adapter \
             {pin}`; its candidates are on: {}; add one on `{pin}` to `runners.{runner}`, or \
             drop the flag",
            listed(candidates)
        ),
        Unservable::UnbuiltPin { pin, runnable } => {
            let runnable = match runnable.is_empty() {
                true => {
                    "(none configured: `runners:` names no adapter this build supports)".to_string()
                }
                false => listed(runnable),
            };
            format!("unknown adapter `{pin}`; this binary can run: {runnable}")
        }
        Unservable::MockBeforeFrozenAdapter => "this run used the mock before runs froze their \
            adapter, and its fixture was not kept; it cannot resume"
            .to_string(),
        Unservable::NoAdapter { built } => format!(
            "this workflow has prompt/loop nodes but `runners:` in the merged config\n\
             names no adapter this binary can run (built: {built}). To exercise this\n\
             workflow with the `mock` adapter instead, declare a test case under\n\
             .yunta/tests/ and run `{}`.",
            command::line(&NextStep::Test { dir: None })
        ),
        Unservable::Unhealthy { adapters } => {
            let unhealthy: Vec<String> = adapters
                .iter()
                .map(|(name, said)| surco::text::detailed(name, said))
                .collect();
            surco::text::problems(
                format!(
                    "adapter health check failed (run `{}` for detail)",
                    command::line(&NextStep::Doctor {
                        sessions: false,
                        full: false
                    })
                ),
                &unhealthy,
            )
        }
    }
}

/// The adapters a runner's candidates are on, comma-separated, or `none` for
/// a runner with no candidate at all.
fn listed(adapters: &[AdapterId]) -> String {
    if adapters.is_empty() {
        return "none".to_string();
    }
    adapters
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a workflow's own check refuses of a run, under the workflow it judged.
pub(super) fn unchecked(workflow: &Arg<String>, errors: &[CheckError]) -> String {
    let words: Vec<String> = errors.iter().map(super::check::line).collect();
    surco::text::problems(super::unrunnable(workflow.clone()), &words)
}

/// What a run another engine drives says: whose it is.
pub fn moving(run: &str, pid: Pid) -> String {
    format!("run {run} is already moving: pid {pid} drives it")
}

/// What a claim says when nothing here can tell a live holder from a leftover:
/// who the lock names, if anyone, and its file, because only a person can tell.
pub fn hold_unclear(run: &str, lock_path: &Path, pid: Option<Pid>) -> String {
    let held = match pid {
        Some(pid) => format!("is held by pid {pid}, a process this user cannot signal"),
        None => "has an engine lock with no readable owner".to_string(),
    };
    format!(
        "run {run} {held} (`{}`): if nothing drives this run, delete that file by hand and \
         resume it again",
        lock_path.display()
    )
}

/// What the inputs a command was handed do not satisfy, in the words a
/// reader sees. An input the command was not given is also named by the flag
/// that would have given it.
pub fn inputs(problem: &InputsProblem) -> String {
    let said = crate::inputs::problem(problem);
    match problem {
        InputsProblem::Missing { name } => format!("{said}; pass `--input {name}=...`"),
        _ => said,
    }
}

/// What a surface's answer was refused for: the refusal, and the claim the
/// answer was given for.
pub fn refused_answer(refused: &Refusal, claim: &Claim, menu: Option<&FailureMenu>) -> String {
    format!(
        "the human surface's answer was refused: {}, for: {}",
        refusal(refused),
        crate::claim::sentence(claim, menu)
    )
}

/// Why an answer does not count as a decision on its escalation, in the words
/// a person reads.
fn refusal(refused: &Refusal) -> String {
    match refused {
        Refusal::OffMenu { chosen, offered } => {
            format!("`{chosen}` is not one of the options offered ({offered})")
        }
        Refusal::Unsaid { chosen, asks } => format!(
            "`{chosen}` asks \"{}\", and the answer says nothing",
            crate::offers::asked(asks)
        ),
        Refusal::Withheld { withheld } => format!(
            "`{}` is withheld: {}",
            withheld.option,
            crate::escalation::because(withheld)
        ),
    }
}

/// Why a decision put to a run is not recorded, in the words of the command
/// that records it.
pub fn gate(refusal: &GateRefusal) -> String {
    match refusal {
        GateRefusal::NotPaused => "this run isn't parked at a pause: a live process may still \
             be driving it, or it already finished"
            .to_string(),
        GateRefusal::NothingToResolve => "this run isn't currently waiting on a decision \
             `resolve_gate` can answer: it's paused for a reason with no menu of options (a failed \
             gate node, an external gate with no forge)"
            .to_string(),
        GateRefusal::UnknownOption { chosen, declared } => {
            format!("option `{chosen}` isn't valid here; declared options: {declared}")
        }
        GateRefusal::Unsaid { chosen, asks } => format!(
            "option `{chosen}` asks \"{}\"; say it with the choice",
            crate::offers::asked(asks)
        ),
        GateRefusal::Withheld { chosen, withheld } => format!(
            "option `{chosen}` is withheld: {}",
            crate::escalation::because(withheld)
        ),
    }
}

/// Why a reply to a node's questions is not recorded: the node asked nothing
/// open, or every way the reply does not answer what it asked.
pub fn answers(refusal: &AnswersRefusal) -> String {
    match refusal {
        AnswersRefusal::NothingAsked { node } => format!(
            "node `{node}` is not waiting on questions: it asked none, or the ones it asked are \
             already answered"
        ),
        AnswersRefusal::Rejected(report) => report.to_string(),
    }
}

/// What a refused workflow is refused for, said before its problems: the
/// check to run on it again, on its file when the refusal knows one.
pub fn unrunnable(workflow: Arg<String>) -> String {
    format!(
        "the workflow fails `{}`",
        command::line(&NextStep::Check { workflow })
    )
}

/// Why a run is not closed, in the words of the command that closes it.
pub fn close(refusal: &CloseRefusal) -> String {
    match refusal {
        CloseRefusal::AlreadyClosed => "the run is already closed".to_string(),
        CloseRefusal::Driven => "an engine is driving the run".to_string(),
        CloseRefusal::Moving => {
            "the run has not stopped, and nothing says the engine that drove it is gone".to_string()
        }
    }
}

/// Why `add` refuses `pack`, installed already at `dest`: the lines that
/// change its ref, or take it out first.
pub fn already_installed(pack: &PackRef, dest: &Path) -> String {
    let update = NextStep::PackUpdate {
        pack: pack.clone(),
        reference: Arg::Blank,
    };
    let remove = NextStep::PackRemove { pack: pack.clone() };
    format!(
        "`{pack}` is already installed at {}; use `{}` to change its ref, or `{}` first",
        dest.display(),
        command::line(&update),
        command::line(&remove)
    )
}

/// Why `update` refuses `pack`, which is not installed: the line that adds it.
pub fn not_installed(pack: &PackRef) -> String {
    let add = NextStep::PackAdd { source: Arg::Blank };
    format!(
        "`{pack}` isn't installed; `{}` it first, not update",
        command::line(&add)
    )
}
