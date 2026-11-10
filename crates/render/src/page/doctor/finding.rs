//! See [`super`]. What one check `doctor` makes found, as data, and the row a
//! person reads it as.

use std::time::Duration;

use surco::components::{Check, Fix, Found};
use yunta_core::caution::Caution;
use yunta_core::check::CheckError;
use yunta_core::step::NextStep;
use yunta_core::{AdapterId, GitHubRepo, PackRef, RunnerCandidate, RunnerName};

use super::session::{session_check, SessionOutcome};
use crate::command;

/// What one check `doctor` makes found, in the order the checks run.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// An adapter `runners:` names, as probing it answered.
    Adapter {
        adapter: AdapterId,
        health: AdapterHealth,
    },
    /// `runners:` names no adapter this build supports: the catalog
    /// workflows that need one, and the runner to declare.
    NoRunner { needing: Vec<String>, declare: Fix },
    /// Who git commits a run's work as.
    Committer { who: String },
    /// git names no one to commit a run's work as: what it said.
    NoCommitter { git_said: String },
    /// The forge the config declares, by its repository, as it answered.
    Forge {
        repo: GitHubRepo,
        answer: ForgeAnswer,
    },
    /// The forge's token is in `token_env`, which is not set: the catalog
    /// workflows that open a pull request through it.
    ForgeTokenUnset {
        repo: GitHubRepo,
        token_env: String,
        opening: Vec<String>,
    },
    /// The remote a run's branch is pushed to, which names no other
    /// repository than the forge's when it holds.
    Remote {
        remote: String,
        repo: GitHubRepo,
        answer: RemoteAnswer,
    },
    /// What the project's checkouts keep of the disk, `kept` as their pool
    /// says it.
    Checkouts { kept: String, keep: CheckoutsKeep },
    /// What the project's checkouts keep could not be read: what stopped it.
    CheckoutsUnread { said: String },
    /// What a new binary's first run costs here over its second, and whether
    /// that slows a suite that links a binary per test target.
    FirstRun {
        first: Duration,
        then: Duration,
        slows_a_suite: bool,
    },
    /// An installed pack whose manifest does not read: what reading it said.
    PackBroken { said: String },
    /// An installed pack's `requires:` this project does not meet.
    PackLacks { pack: PackRef, lack: PackLack },
    /// A workflow of an installed pack that a run would be refused here, and
    /// what fixes it, when this repository answers for it.
    PackRefused {
        pack: PackRef,
        error: CheckError,
        fix: Option<Fix>,
    },
    /// A workflow of an installed pack that a run would be cautioned about
    /// here.
    PackCaution { pack: PackRef, caution: Caution },
    /// `--session` found no binding whose adapter answered healthy.
    NoBinding,
    /// One binding's session, opened and driven: the binding, each runner
    /// that names it and whether first, and how its session ended.
    Session {
        candidate: RunnerCandidate,
        named_by: Vec<(RunnerName, bool)>,
        outcome: SessionOutcome,
    },
}

/// What probing an adapter answered.
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterHealth {
    /// It answered, with the version its CLI reported when it reported one.
    Healthy { version: Option<String> },
    /// It did not: what it, or probing it, said.
    Unhealthy { said: String },
}

/// What the forge answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeAnswer {
    /// It answered, and its token can push.
    Reachable,
    /// It answered, and its token cannot push there.
    CannotPush,
    /// It did not answer: what reaching it said.
    Unanswered { said: String },
}

/// Why the remote a run's branch is pushed to does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteAnswer {
    /// It is there, at `url`, which names another repository.
    Elsewhere { url: String },
    /// There is no such remote.
    Missing,
    /// It could not be read: what git said.
    Unread { detail: String },
}

/// How what the project's checkouts keep stands against the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutsKeep {
    /// Within the builds' budget, over the part of the disk kept free.
    Within,
    /// Their builds are over their budget: the next checkout taken empties
    /// the ones past it.
    BuildsOverBudget,
    /// The disk is already under the part kept free, under which a run
    /// stops.
    DiskUnderFree,
}

/// What an installed pack's `requires:` names that this project lacks.
#[derive(Debug, Clone, PartialEq)]
pub enum PackLack {
    /// A runner `runners:` does not define, or defines with no candidate,
    /// and the runner to declare.
    Runner { runner: RunnerName, declare: Fix },
    /// Permissions on a runner in the form the engine retired: what it
    /// says of them.
    RetiredPermissions { said: String },
    /// An `mcp_server` not defined under `mcp_servers:`.
    McpServer { server: String },
    /// A program not found on `PATH`.
    Program { program: String },
}

impl Finding {
    /// The row the finding is, as a person reads it.
    pub fn check(&self) -> Check {
        match self {
            Finding::Adapter { adapter, health } => adapter_check(adapter, health),
            Finding::NoRunner { needing, declare } => {
                let check = match needing.is_empty() {
                    true => Check::new(
                        Found::Caution,
                        "runners",
                        "none declared: a workflow's agent nodes cannot run until one is",
                    ),
                    false => Check::new(
                        Found::Problem,
                        "runners",
                        format!(
                            "none declared, and {} {} one",
                            surco::text::listed(needing.iter().map(String::as_str)),
                            surco::text::agreeing(needing.len(), "needs", "need")
                        ),
                    ),
                };
                check.fix(declare.clone())
            }
            Finding::Committer { who } => {
                Check::new(Found::Holds, "git", format!("commits as {who}"))
            }
            Finding::NoCommitter { git_said } => Check::new(
                Found::Problem,
                "git",
                crate::refusal::unnamed_committer(git_said),
            ),
            Finding::Forge { repo, answer } => forge_check(repo, answer),
            Finding::ForgeTokenUnset {
                repo,
                token_env,
                opening,
            } => token_unset_check(repo, token_env, opening),
            Finding::Remote {
                remote,
                repo,
                answer,
            } => {
                let said = match answer {
                    RemoteAnswer::Elsewhere { url } => {
                        format!("remote `{remote}` is {url}, not {repo}")
                    }
                    RemoteAnswer::Missing => {
                        format!("no remote `{remote}` to push a run's branch to")
                    }
                    RemoteAnswer::Unread { detail } => {
                        format!("the remote `{remote}` could not be read: {detail}")
                    }
                };
                Check::new(Found::Problem, "forge", said)
            }
            Finding::Checkouts { kept, keep } => checkouts_check(kept, *keep),
            Finding::CheckoutsUnread { said } => {
                Check::new(Found::Caution, "checkouts", said.as_str())
            }
            Finding::FirstRun {
                first,
                then,
                slows_a_suite,
            } => first_run_check(*first, *then, *slows_a_suite),
            Finding::PackBroken { said } => Check::new(Found::Problem, "pack", said.as_str()),
            Finding::PackLacks { pack, lack } => pack_lack_check(pack, lack),
            Finding::PackRefused { pack, error, fix } => {
                let (said, remedy) = match crate::refusal::check::lacking(error) {
                    Some((fact, remedy)) => (fact, Some(Fix::said(remedy))),
                    None => (crate::refusal::check::line(error), None),
                };
                Check {
                    fix: fix.clone().or(remedy),
                    ..Check::new(Found::Problem, format!("pack {pack}"), said)
                }
            }
            Finding::PackCaution { pack, caution } => Check::new(
                Found::Caution,
                format!("pack {pack}"),
                crate::caution::line(caution),
            ),
            Finding::NoBinding => Check::new(
                Found::Caution,
                "sessions",
                "no binding to open a session on",
            ),
            Finding::Session {
                candidate,
                named_by,
                outcome,
            } => session_check(candidate, named_by, outcome),
        }
    }
}

/// What probing `adapter` answered, as its row: a healthy one folds as its
/// id and the version number it reported.
fn adapter_check(adapter: &AdapterId, health: &AdapterHealth) -> Check {
    match health {
        AdapterHealth::Healthy { version } => {
            let said = version
                .as_deref()
                .map(|v| format!(" ({v})"))
                .unwrap_or_default();
            Check::new(Found::Holds, adapter.as_str(), format!("healthy{said}"))
                .brief(versioned(adapter, version.as_deref()))
        }
        AdapterHealth::Unhealthy { said } => Check::new(
            Found::Problem,
            adapter.as_str(),
            format!("unhealthy: {said}"),
        ),
    }
}

/// What names a healthy adapter where the checks that hold fold: its id,
/// and the version number its CLI reported, when it reported one.
pub(super) fn versioned(adapter: &AdapterId, version: Option<&str>) -> String {
    let number = version.and_then(|version| {
        version
            .split_whitespace()
            .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))
    });
    match number {
        Some(number) => format!("{adapter} {number}"),
        None => adapter.to_string(),
    }
}

/// What the forge of `repo` answered, as its row.
fn forge_check(repo: &GitHubRepo, answer: &ForgeAnswer) -> Check {
    let named = format!("github {repo}");
    match answer {
        ForgeAnswer::Reachable => {
            Check::new(Found::Holds, "forge", format!("{named}: reachable")).brief(named.as_str())
        }
        ForgeAnswer::CannotPush => Check::new(
            Found::Problem,
            "forge",
            format!("{named}: reachable, and its token cannot push there"),
        ),
        ForgeAnswer::Unanswered { said } => {
            Check::new(Found::Problem, "forge", format!("{named}: {said}"))
        }
    }
}

/// A forge whose token variable, `token_env`, is not set here. Without the
/// token a gate published to the forge falls back to the console; only a
/// `pull_request` node stops. So the missing token is a problem only when
/// `opening` names a workflow, and a caution otherwise.
fn token_unset_check(repo: &GitHubRepo, token_env: &str, opening: &[String]) -> Check {
    let unset = format!("github {repo}: `{token_env}`, the variable its token is in, is not set");
    let export = Fix::said(format!("export {token_env} to reach it"));
    if opening.is_empty() {
        return Check::new(
            Found::Caution,
            "forge",
            format!("{unset}, and no workflow here opens a pull request"),
        )
        .so("a gate published there asks on the console instead")
        .fix(export);
    }
    Check::new(
        Found::Problem,
        "forge",
        format!(
            "{unset}, and {} {} a pull request through it",
            surco::text::listed(opening.iter().map(String::as_str)),
            surco::text::agreeing(opening.len(), "opens", "open"),
        ),
    )
    .fix(export)
}

/// What the checkouts keep, as its row: a problem when the disk is already
/// under the part kept free, and a caution when their builds are over their
/// budget, each with what to do about it.
pub(super) fn checkouts_check(kept: &str, keep: CheckoutsKeep) -> Check {
    let check = |found| Check::new(found, "checkouts", kept);
    match keep {
        CheckoutsKeep::DiskUnderFree => {
            check(Found::Problem).fix(Fix::said(crate::pause::make_room()))
        }
        CheckoutsKeep::BuildsOverBudget => check(Found::Caution)
            .so("the next checkout taken empties the builds past the budget")
            .fix(Fix::said(format!(
                "run `{}` to empty them now",
                command::line(&NextStep::Gc)
            ))),
        CheckoutsKeep::Within => check(Found::Holds),
    }
}

/// What a new binary's first run took, `first`, and its second, `then`, as
/// its row: when that slows a suite, why and how to exempt the terminal.
pub(super) fn first_run_check(first: Duration, then: Duration, slows_a_suite: bool) -> Check {
    let said = format!(
        "a new binary's first run takes {} here, its next {}",
        millis(first),
        millis(then)
    );
    match slows_a_suite {
        false => Check::new(Found::Holds, "first run", said),
        true => Check::new(Found::Caution, "first run", said)
            .so(
                "macOS assesses each new executable before its first run, and a suite pays \
                 that for every test binary it links again",
            )
            .fix(Fix::said(
                "add the terminal that runs yunta under System Settings > Privacy & Security \
                 > Developer Tools, then open it again",
            )),
    }
}

fn millis(spent: Duration) -> String {
    format!("{} ms", spent.as_millis())
}

/// What an installed pack's `requires:` lacks here, as its row.
fn pack_lack_check(pack: &PackRef, lack: &PackLack) -> Check {
    let subject = format!("pack {pack}");
    match lack {
        PackLack::Runner { runner, declare } => Check::new(
            Found::Problem,
            subject,
            format!(
                "requires runner `{runner}`, which `runners:` does not define, or defines with \
                 zero candidates"
            ),
        )
        .fix(declare.clone()),
        PackLack::RetiredPermissions { said } => Check::new(Found::Problem, subject, said.as_str()),
        PackLack::McpServer { server } => Check::new(
            Found::Problem,
            subject,
            format!("requires mcp_server `{server}`, not defined under `mcp_servers:`"),
        ),
        PackLack::Program { program } => Check::new(
            Found::Problem,
            subject,
            format!("requires program `{program}`, not found on PATH"),
        ),
    }
}
