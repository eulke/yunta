//! See [`super`]. A repository prepared for Yunta once: its ecosystem, test
//! command, base branch and the adapter CLIs this machine answers for are
//! detected, then `.yunta/config.yaml`, defensive `.gitignore` entries and
//! the mechanism skill are written. Every value is the best one detected; a
//! caller that asks a person hands in what they answered instead.

use std::path::Path;

use yunta_core::refusal::SetupRefusal;
use yunta_core::step::NextStep;
use yunta_core::{Detected, ProbedAdapter};
use yunta_engine::git::Cause;
use yunta_engine::process::Supervision;
use yunta_render::command;
use yunta_render::refusal::Refused;

use super::skill::{self, MECHANISM_SKILL_DIR};
use super::SetupUnwritten;
use crate::{Error, Failure, Stop, Workspace};

impl Workspace {
    /// What `init` would write for the repository this workspace works in:
    /// the project's name and base branch as detected, and what the
    /// repository runs. Refused when the project's config is there already
    /// and `force` does not say to write it again.
    pub async fn detect_project(
        &self,
        force: bool,
        stop: &Stop,
    ) -> Result<DetectedProject<'_>, Error> {
        let config = self.dir.join(".yunta/config.yaml");
        if config.exists() && !force {
            return Err(Refused::Setup(SetupRefusal::Exists { path: config }).into());
        }
        let name = self
            .dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "workflow-project".to_string());
        let base_branch = base_branch(&self.dir, self.supervision(stop)).await?;
        let detected = crate::machine::in_repo(self, &self.dir, stop).await?;
        Ok(DetectedProject {
            workspace: self,
            name,
            base_branch,
            detected,
            force,
        })
    }
}

/// A repository's project as `init` would write it, before anything is
/// written: the name and base branch it detected, and what the repository
/// runs.
pub struct DetectedProject<'w> {
    workspace: &'w Workspace,
    pub name: String,
    pub base_branch: String,
    pub detected: Detected,
    force: bool,
}

/// What `init` wrote, and what it left as it was, with what it detected and
/// the adapters this machine answered for.
pub struct InitializedProject {
    pub wrote: Vec<String>,
    pub skipped: Vec<String>,
    pub detected: Detected,
    pub probed: Vec<ProbedAdapter>,
}

impl DetectedProject<'_> {
    /// Writes the project as `name` on `base_branch`: its config, from what
    /// the repository runs and the adapters this machine answers for, parsed
    /// back before it is written; the ignore entries `init` adds once; and
    /// the mechanism skill, written again only under `force`.
    pub async fn write(self, name: &str, base_branch: &str) -> Result<InitializedProject, Error> {
        let probed = crate::machine::probe_known_adapters().await;
        let config_yaml = config_yaml(name, base_branch, &probed, &self.detected);
        let repo = &self.workspace.dir;
        let (wrote, skipped) = write_project(repo, config_yaml, self.force).await?;
        Ok(InitializedProject {
            wrote,
            skipped,
            detected: self.detected,
            probed,
        })
    }
}

/// The base branch a project starts from: the remote's default, else the
/// branch checked out here, else `main`. Only git's "no remote HEAD" answer,
/// or a remote HEAD that names no branch of `origin`, falls through to the
/// next probe. Any other failure stops `init`, since a
/// base read from a git that never answered would be written as if found.
async fn base_branch(repo: &Path, supervision: Supervision<'_>) -> Result<String, Error> {
    // `-q` makes git exit 1, silently, when there is no remote HEAD to read.
    let remote = yunta_engine::git::read(
        repo,
        &["symbolic-ref", "-q", "refs/remotes/origin/HEAD"],
        supervision,
    )
    .await;
    match remote {
        Ok(raw) => {
            if let Some(branch) = raw.trim().strip_prefix("refs/remotes/origin/") {
                if !branch.is_empty() {
                    return Ok(branch.to_string());
                }
            }
        }
        Err(error) if matches!(error.cause, Cause::Exit { code: Some(1), .. }) => {}
        Err(error) => return Err(error.into()),
    }
    let current = yunta_engine::git::read(repo, &["branch", "--show-current"], supervision).await?;
    let name = current.trim();
    if name.is_empty() {
        return Ok("main".to_string());
    }
    Ok(name.to_string())
}

/// The project's config as `init` writes it: its name and base branch, what
/// the repository was detected to run, and the runners it could name, with
/// each adapter this machine answered for, healthy or not.
fn config_yaml(
    project_name: &str,
    base_branch: &str,
    probed: &[ProbedAdapter],
    detected: &Detected,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Written by `{}`: team-shared, commit this file.\n",
        command::line(&NextStep::Init)
    ));
    out.push_str("# Personal overrides belong in ~/.yunta/config.yaml (the user\n");
    out.push_str("# config layer), never in this file.\n\n");
    out.push_str("project:\n");
    out.push_str(&format!("  name: {}\n", yaml_scalar(project_name)));
    out.push_str(&format!("  base_branch: {base_branch}\n"));
    out.push_str("  branch_prefix: yunta/\n\n");
    out.push_str(&detected_yaml(detected));

    out.push_str("# `runners:` names roles your workflows' `runner:` fields reference,\n");
    out.push_str("# each with one or more adapter candidates (first capable one wins).\n");
    for adapter in probed {
        if adapter.healthy {
            out.push_str(&format!(
                "# detected: {} ({})\n",
                adapter.id, adapter.detail
            ));
        } else {
            out.push_str(&format!(
                "# not available: {} ({})\n",
                adapter.id, adapter.detail
            ));
        }
    }
    out.push_str("# runners:\n");
    out.push_str("#   implementer:\n");
    if let Some(healthy) = probed.iter().find(|a| a.healthy) {
        out.push_str(&format!(
            "#     - {{ adapter: {}, model: <model-name> }}\n",
            healthy.id
        ));
    } else {
        out.push_str(&format!(
            "#     - {{ adapter: {}, model: <model-name> }}\n",
            crate::machine::first_built_adapter()
        ));
    }
    out
}

/// What `init` found in the repository, as config: the commands a
/// workflow asks for by name, the suite a run measures when it reads the
/// measurement, and the forge `origin` is on.
fn detected_yaml(detected: &Detected) -> String {
    let mut out = String::new();
    if !detected.commands.is_empty() {
        out.push_str("# What this project runs for each capability a workflow names\n");
        out.push_str(&format!(
            "# (`run: {{ command: lint }}`), as `{}` detected it.\n",
            command::line(&NextStep::Init)
        ));
        out.push_str("commands:\n");
        for (name, text) in &detected.commands {
            out.push_str(&format!("  {name}: {}\n", yaml_string(text)));
        }
        out.push('\n');
    }
    if let Some(suite) = &detected.suite {
        out.push_str("# Measured before a run's first node when the run reads it.\n");
        out.push_str(&format!("baseline:\n  suite: {}\n\n", yaml_string(suite)));
    }
    if let Some(repo) = &detected.forge {
        out.push_str("# Where a `pull_request` node opens its pull request.\n");
        out.push_str(&format!(
            "forge:\n  github:\n    repo: {repo}\n    token_env: GITHUB_TOKEN\n\n"
        ));
    }
    out
}

/// `text` as YAML writes a string: as it is when nothing in it reads as
/// anything else, quoted when a comment, a mapping or another type would cut
/// or change it.
fn yaml_scalar(text: &str) -> String {
    yunta_core::yaml::to_string(&text)
        .map(|written| written.trim_end().to_string())
        .unwrap_or_else(|_| yaml_string(text))
}

/// `text` as a double-quoted YAML scalar.
fn yaml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The line `init` writes above the entries it adds to `.gitignore`, and
/// finds again so a second `init` adds nothing.
fn gitignore_marker() -> String {
    format!("# added by `{}`", command::line(&NextStep::Init))
}

/// Defensive only: `paths.runs`/`paths.worktrees` default to `~/.yunta`,
/// outside the repo entirely, so none of this exists in a repo with
/// default config. It's here for the day someone points `paths:` back
/// into the repo on purpose.
fn gitignore_block() -> String {
    format!(
        "\n{marker}: only matters if `paths:` ever points inside \
         this repo (defaults keep run state in ~/.yunta):\n.yunta/runs/\n\
         .yunta/worktrees/\n.yunta/*.db\n.yunta/*.db-*\n",
        marker = gitignore_marker()
    )
}

/// Adds `init`'s entries to `repo`'s `.gitignore`, once: `false` when they
/// are there already.
fn write_gitignore(repo: &Path) -> std::io::Result<bool> {
    let path = repo.join(".gitignore");
    // An absent `.gitignore` is the create case; any *other* read error
    // (unreadable, not UTF-8, a directory) is propagated, never swallowed
    // as "empty": appending to an empty string would then overwrite a
    // file whose real content was not read.
    let existing = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if existing.contains(&gitignore_marker()) {
        return Ok(false);
    }
    let mut updated = existing;
    updated.push_str(&gitignore_block());
    std::fs::write(&path, updated)?;
    Ok(true)
}

/// Writes what `init` puts in the project: the config first, then the
/// ignore entries and the mechanism skill. Answers what it wrote, and what
/// it left as it was.
async fn write_project(
    repo: &Path,
    config_yaml: String,
    force: bool,
) -> Result<(Vec<String>, Vec<String>), Error> {
    let mut wrote = Vec::new();
    let mut skipped = Vec::new();
    let config_path = repo.join(".yunta/config.yaml");
    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| unwritten("create", &parent.display(), source))?;
    }
    // Never write a config this binary can't read back: parse the
    // generated text into a real `ConfigLayer` first, so a broken template
    // fails here instead of leaving an unreadable file, the more so under
    // `--force`, which would otherwise overwrite a good config with a bad
    // one.
    yunta_core::ConfigLayer::read(&config_yaml)
        .map_err(|error| Failure::new(GeneratedConfigInvalid(error.to_string())))?;
    tokio::fs::write(&config_path, config_yaml)
        .await
        .map_err(|source| unwritten("write", &config_path.display(), source))?;
    wrote.push(".yunta/config.yaml".to_string());

    match write_gitignore(repo) {
        Ok(true) => wrote.push(".gitignore".to_string()),
        Ok(false) => skipped.push(".gitignore (already has yunta entries)".to_string()),
        Err(source) => return Err(unwritten("update", &".gitignore", source)),
    }

    match skill::write_mechanism_skill(repo, force) {
        Ok(true) => wrote.push(format!("{MECHANISM_SKILL_DIR}/SKILL.md")),
        Ok(false) => skipped.push(format!(
            "{MECHANISM_SKILL_DIR}/SKILL.md (already exists, pass --force to rewrite)"
        )),
        Err(source) => return Err(unwritten("write", &"the mechanism skill", source)),
    }
    Ok((wrote, skipped))
}

/// A step of writing the project the disk refused: what it was doing, to
/// what.
fn unwritten(verb: &'static str, what: &dyn std::fmt::Display, source: std::io::Error) -> Error {
    Failure::new(SetupUnwritten {
        doing: format!("{verb} {what}"),
        source,
    })
    .into()
}

/// A config `init` generated that does not read back: a bug in this binary,
/// caught before the file is written.
#[derive(Debug, thiserror::Error)]
#[error("the generated config is not valid: {0}")]
struct GeneratedConfigInvalid(String);

#[cfg(test)]
mod tests;
