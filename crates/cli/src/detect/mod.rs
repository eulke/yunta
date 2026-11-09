//! Reads what a repository says about itself before anyone configures
//! Yunta for it — the commands its ecosystem answers for lint, tests and
//! the rest, the suite a run can measure, and the forge its `origin` is on
//! — and the adapter CLIs this machine answers for, into core's
//! [`Detected`].
//!
//! Detection only proposes. `yunta init` writes what it finds into the
//! config a person commits, and `check`, `doctor` and `pack add` name it
//! beside a key a workflow needs and the config lacks, in render's
//! `declare` words — no run reads a detected value.

mod adapters;
mod ecosystems;
mod js;
mod remote;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;

use surco::components::Fix;
use yunta_core::{CommandName, ConfigKey, Detected, Ecosystem, MachineAdapters, RunnerName};
use yunta_engine::process::Supervision;
use yunta_render::declare;

pub(crate) use adapters::probe_known_adapters;

/// What `repo`'s own files say — everything but its remote.
pub(crate) fn in_files(repo: &Path) -> Detected {
    let found = ecosystems::rust(repo)
        .or_else(|| js::node(repo))
        .or_else(|| ecosystems::go(repo))
        .or_else(|| ecosystems::python(repo));
    match found {
        Some(found) => Detected {
            ecosystem: Some(found.ecosystem),
            commands: found.commands,
            suite: found.suite,
            forge: None,
            adapters: MachineAdapters::default(),
        },
        None => Detected::default(),
    }
}

/// What `repo` says, the forge its `origin` is on included. A stop while
/// git read the remote is the caller's stop, not a Detected without one.
pub(crate) async fn in_repo(
    repo: &Path,
    supervision: Supervision<'_>,
) -> Result<Detected, yunta_engine::GitError> {
    Ok(Detected {
        forge: remote::github_repo(repo, supervision).await?,
        ..in_files(repo)
    })
}

/// The adapters this machine answers for when `errors` name a runner the
/// config lacks — the one suggestion that needs them — and none asked for
/// otherwise.
pub(crate) async fn adapters_for(errors: &[yunta_engine::CheckError]) -> MachineAdapters {
    match wants_runner(errors) {
        true => MachineAdapters::probed(&probe_known_adapters().await),
        false => MachineAdapters::default(),
    }
}

/// What this repository answers for each key `errors` says a workflow
/// needs and the config lacks, each once, in the order they are named.
pub(crate) fn suggestions(errors: &[yunta_engine::CheckError], detected: &Detected) -> Vec<Fix> {
    let mut said: Vec<Fix> = Vec::new();
    for fix in errors.iter().filter_map(|error| answered(error, detected)) {
        if !said.contains(&fix) {
            said.push(fix);
        }
    }
    said.extend(runner_fix(errors, detected));
    said
}

/// What fixes `error` on its own: what this repository answers for the
/// key it says a workflow lacks, or the runner to declare for it; `None`
/// when nothing here does.
pub(crate) fn fix_for(error: &yunta_engine::CheckError, detected: &Detected) -> Option<Fix> {
    answered(error, detected).or_else(|| runner_fix(std::slice::from_ref(error), detected))
}

/// Whether `errors` say a node names a runner the config lacks, or needs
/// the default runner it lacks.
fn wants_runner(errors: &[yunta_engine::CheckError]) -> bool {
    !runners_wanted(errors).is_empty() || default_wanted(errors)
}

/// The runner to declare for `errors`, on the adapters this machine
/// answers for, when they want one.
fn runner_fix(errors: &[yunta_engine::CheckError], detected: &Detected) -> Option<Fix> {
    wants_runner(errors).then(|| {
        declare::runners(
            &runners_wanted(errors),
            default_wanted(errors),
            &detected.adapters,
        )
    })
}

/// What this repository answers for the key `error` says a workflow needs
/// and the config lacks, when it answers it.
fn answered(error: &yunta_engine::CheckError, detected: &Detected) -> Option<Fix> {
    let key = match error {
        yunta_engine::CheckError::Unset { key, .. } => key.clone(),
        yunta_engine::CheckError::BaselineWithoutSuite { .. } => ConfigKey::BaselineSuite,
        _ => return None,
    };
    declare::as_detected(detected, &key).map(Fix::said)
}

/// The runners `errors` say a node names and the config does not
/// declare, or declares with no candidate — each once, in order. A name
/// one slip from a runner the config declares is a typo to fix, not a
/// runner to declare.
fn runners_wanted(errors: &[yunta_engine::CheckError]) -> Vec<RunnerName> {
    let mut roles: Vec<RunnerName> = Vec::new();
    for error in errors {
        if let yunta_engine::CheckError::UnknownRunner {
            runner, near: None, ..
        }
        | yunta_engine::CheckError::RunnerHasNoCandidates { runner, .. } = error
        {
            if !roles.contains(runner) {
                roles.push(runner.clone());
            }
        }
    }
    roles
}

/// Whether `errors` say a node names no runner and the config declares no
/// `defaults.runner` for it.
fn default_wanted(errors: &[yunta_engine::CheckError]) -> bool {
    errors.iter().any(|error| {
        matches!(
            error,
            yunta_engine::CheckError::Unset {
                key: ConfigKey::Runner,
                ..
            }
        )
    })
}

/// What one ecosystem answered for a repository.
pub(super) struct Found {
    pub(super) ecosystem: Ecosystem,
    pub(super) commands: BTreeMap<CommandName, String>,
    pub(super) suite: Option<String>,
}

/// `pairs` as the commands a repository answers with.
pub(super) fn commands(pairs: &[(&'static str, String)]) -> BTreeMap<CommandName, String> {
    pairs
        .iter()
        .map(|(name, text)| (CommandName::from_static(name), text.clone()))
        .collect()
}
