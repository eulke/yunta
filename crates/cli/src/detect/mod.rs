//! What a repository says about itself before anyone configures Yunta
//! for it: the commands its ecosystem answers for lint, tests and the
//! rest, the suite a run can measure, and the forge its `origin` is on.
//!
//! Detection only proposes. `yunta init` writes what it finds into the
//! config a person commits, and `check`, `doctor` and `pack add` name it
//! beside a key a workflow needs and the config lacks — no run reads a
//! detected value.

mod ecosystems;
mod js;
mod remote;

use std::collections::BTreeMap;
use std::path::Path;

use yunta_core::{CommandName, ConfigKey, GitHubRepo};
use yunta_engine::process::Supervision;

pub(crate) use ecosystems::Ecosystem;

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
            },
            None => Detected::default(),
        }
    }

    /// What `repo` says, the forge its `origin` is on included.
    pub(crate) async fn in_repo(repo: &Path, supervision: Supervision<'_>) -> Self {
        Detected {
            forge: remote::github_repo(repo, supervision).await,
            ..Detected::in_files(repo)
        }
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
pub(crate) fn suggestions(errors: &[yunta_engine::CheckError], detected: &Detected) -> Vec<String> {
    let mut said: Vec<String> = Vec::new();
    for error in errors {
        let key = match error {
            yunta_engine::CheckError::Unset { key, .. } => key.clone(),
            yunta_engine::CheckError::BaselineWithoutSuite { .. } => ConfigKey::BaselineSuite,
            _ => continue,
        };
        if let Some(line) = detected.suggestion(&key) {
            if !said.contains(&line) {
                said.push(line);
            }
        }
    }
    said
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
