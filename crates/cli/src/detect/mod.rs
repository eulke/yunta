//! What a repository says about itself before anyone configures Yunta
//! for it: the commands its ecosystem answers for lint, tests and the
//! rest, the suite a run can measure, and the forge its `origin` is on.
//!
//! Detection only proposes. `yunta init` writes what it finds into the
//! config a person commits, and `check`, `doctor` and `pack add` name it
//! beside a key a workflow needs and the config lacks — no run reads a
//! detected value.

mod adapters;
mod ecosystems;
mod js;
mod remote;

use std::collections::BTreeMap;
use std::path::Path;

use surco::components::{Prose, Said};
use surco::doc::Block;
use yunta_core::{AdapterId, CommandName, ConfigKey, GitHubRepo, RunnerName};
use yunta_engine::process::Supervision;

pub(crate) use adapters::{healthy, probe_known_adapters, runner_step, ProbedAdapter};
pub(crate) use ecosystems::Ecosystem;

/// What this project could declare, as a person is told it: the sentence
/// that says what, and the lines to paste when the sentence does not carry
/// them itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declaration {
    pub(crate) said: String,
    pub(crate) paste: Vec<String>,
}

impl Declaration {
    /// A declaration its sentence says whole.
    fn said(said: String) -> Self {
        Declaration {
            said,
            paste: Vec::new(),
        }
    }

    /// The blocks a page draws it with: the sentence as the heading the
    /// lines to paste hang from, or as prose when there is nothing to paste.
    pub(crate) fn blocks(&self) -> Vec<Block> {
        match self.paste.is_empty() {
            true => vec![Prose(self.said.clone()).into()],
            false => vec![
                Block::Heading(self.said.clone()),
                Said(self.paste.clone()).into(),
            ],
        }
    }

    /// As plain text says it: the sentence, then each line to paste one
    /// step in.
    pub(crate) fn lines(&self) -> Vec<String> {
        std::iter::once(self.said.clone())
            .chain(self.paste.iter().map(|line| format!("  {line}")))
            .collect()
    }
}

/// What one repository was found to have.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Detected {
    pub(crate) ecosystem: Option<Ecosystem>,
    /// What the repository runs for each capability, by the name a
    /// workflow asks for it with.
    pub(crate) commands: BTreeMap<CommandName, String>,
    /// The suite a run measures before its first node.
    pub(crate) suite: Option<String>,
    /// The GitHub repository `origin` names.
    pub(crate) forge: Option<GitHubRepo>,
    /// The adapter CLIs that answered healthy here, when something asked
    /// — probing spawns each one, so only a caller with a runner to
    /// propose does.
    pub(crate) adapters: Vec<AdapterId>,
}

impl Detected {
    /// What `repo`'s own files say — everything but its remote.
    pub(crate) fn in_files(repo: &Path) -> Self {
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
                adapters: Vec::new(),
            },
            None => Detected::default(),
        }
    }

    /// What `repo` says, the forge its `origin` is on included. A stop while
    /// git read the remote is the caller's stop, not a Detected without one.
    pub(crate) async fn in_repo(
        repo: &Path,
        supervision: Supervision<'_>,
    ) -> Result<Self, yunta_engine::GitError> {
        Ok(Detected {
            forge: remote::github_repo(repo, supervision).await?,
            ..Detected::in_files(repo)
        })
    }

    /// The same, with the adapters this machine answers for when
    /// `errors` name a runner the config lacks — the one suggestion that
    /// needs them.
    pub(crate) async fn for_errors(mut self, errors: &[yunta_engine::CheckError]) -> Self {
        if !runners_wanted(errors).is_empty() || default_wanted(errors) {
            self.adapters = healthy(&probe_known_adapters().await);
        }
        self
    }

    /// What to declare for `key`, as this repository answers it — `None`
    /// when nothing here does.
    pub(crate) fn suggestion(&self, key: &ConfigKey) -> Option<String> {
        match key {
            ConfigKey::Command { command } => self.commands.get(command).map(|text| {
                format!("detected here: declare `commands: {{ {command}: \"{text}\" }}`")
            }),
            ConfigKey::BaselineSuite => self.suite.as_ref().map(|suite| {
                format!("detected here: declare `baseline: {{ suite: \"{suite}\" }}`")
            }),
            ConfigKey::Forge => self.forge.as_ref().map(|repo| {
                format!(
                    "detected here: declare `forge: {{ github: {{ repo: {repo}, token_env: \
                     GITHUB_TOKEN }} }}`"
                )
            }),
            ConfigKey::Coverage
            | ConfigKey::Executor { .. }
            | ConfigKey::Runner
            | ConfigKey::RunBranch => None,
        }
    }
}

/// What this repository answers for each key `errors` says a workflow
/// needs and the config lacks, each once, in the order they are named.
pub(crate) fn suggestions(
    errors: &[yunta_engine::CheckError],
    detected: &Detected,
) -> Vec<Declaration> {
    let mut said: Vec<Declaration> = Vec::new();
    for error in errors {
        let key = match error {
            yunta_engine::CheckError::Unset { key, .. } => key.clone(),
            yunta_engine::CheckError::BaselineWithoutSuite { .. } => ConfigKey::BaselineSuite,
            _ => continue,
        };
        if let Some(line) = detected.suggestion(&key) {
            let declaration = Declaration::said(line);
            if !said.contains(&declaration) {
                said.push(declaration);
            }
        }
    }
    let roles = runners_wanted(errors);
    let needs_default = default_wanted(errors);
    if !roles.is_empty() || needs_default {
        said.push(runner_step(&roles, needs_default, &detected.adapters));
    }
    said
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
