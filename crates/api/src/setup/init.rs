//! See [`super`]. A repository prepared for Yunta once: its ecosystem, test
//! command, base branch and the adapter CLIs this machine answers for are
//! detected, and the runner roles its workflows name proposed; then
//! `.yunta/config.yaml`, defensive `.gitignore` entries and the mechanism
//! skill are written. Every value is the best one detected; a caller that asks
//! a person hands in what they answered instead, the runners included: each
//! role a person put on an adapter with a model is written declared, and the
//! rest stay proposed, commented, as `init` writes them unasked. The config's YAML is
//! `config_yaml`'s.

mod config_yaml;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use yunta_core::refusal::SetupRefusal;
use yunta_core::step::NextStep;
use yunta_core::{
    AdapterId, ConfigKey, Detected, ModelName, ProbedAdapter, RunnerCandidate, RunnerName, Workflow,
};
use yunta_engine::git::Cause;
use yunta_engine::process::Supervision;
use yunta_render::command;
use yunta_render::refusal::Refused;

use super::skill::{self, MECHANISM_SKILL_DIR};
use super::SetupUnwritten;
use crate::{Error, Failure, Stop, Workspace};
use config_yaml::config_yaml;

impl Workspace {
    /// What `init` would write for the repository this workspace works in:
    /// the project's name and base branch as detected, and what the
    /// repository runs. Refused when the project's config is there already
    /// and `force` does not say to write it again.
    pub async fn detect_project(&self, force: bool, stop: &Stop) -> Result<DetectedProject, Error> {
        let config = crate::project::repository_layer(&self.dir);
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
        let probed = crate::machine::probe_known_adapters(&self.env.path).await;
        let workflows: Vec<Workflow> = self
            .catalog_workflows()
            .into_iter()
            .map(|(_, workflow)| workflow)
            .collect();
        let draft = ProjectDraft {
            repo: self.dir.clone(),
            name,
            base_branch,
            detected,
            probed,
            proposed: ProposedRunners::of(&workflows),
        };
        Ok(DetectedProject {
            workspace: self.clone(),
            draft,
            force,
        })
    }
}

/// The runner roles a repository's setup proposes: those its workflows name, or
/// the one proposed to a project whose workflows name none; and whether a node
/// names no runner, so `defaults.runner` names the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedRunners {
    pub roles: Vec<RunnerName>,
    pub needs_default: bool,
}

impl ProposedRunners {
    /// What `workflows` propose, each role once, in the order their nodes name
    /// them.
    fn of(workflows: &[Workflow]) -> Self {
        let mut roles: Vec<RunnerName> = Vec::new();
        let mut needs_default = false;
        for node in workflows.iter().flat_map(Workflow::iter_nodes) {
            for role in node.runner.iter().chain(&node.runners) {
                if !roles.contains(role) {
                    roles.push(role.clone());
                }
            }
            needs_default |= ConfigKey::needed_by(node).contains(&ConfigKey::Runner);
        }
        if roles.is_empty() {
            roles.push(RunnerName::PROPOSED);
        }
        ProposedRunners {
            roles,
            needs_default,
        }
    }
}

/// What a project is set up as: the name and base branch it is written under, and
/// how its runners are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSetup {
    pub name: String,
    pub base_branch: String,
    pub runners: RunnerDeclaration,
}

/// How setting a project up writes its runners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerDeclaration {
    /// Commented out, each proposed role on the first adapter that answered, its
    /// model left blank: what `init` writes when nobody answers for them.
    Proposed,
    /// As a person answered: each role on the adapter they picked, with the model
    /// they typed. A proposed role missing here stays proposed.
    Declared(BTreeMap<RunnerName, RunnerCandidate>),
}

/// A repository's project as `init` would write it, bound to the repository it is
/// written into: what was detected, and whether a config already there is written
/// again. Not its [`ProjectDraft`], what was detected alone, which any holder can keep.
pub struct DetectedProject {
    workspace: Workspace,
    pub draft: ProjectDraft,
    force: bool,
}

/// A repository's project as `init` would write it, before anything is written: the
/// repository, the name and base branch it detected, what the repository runs, the
/// adapters this machine answered for, and the runner roles it proposes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectDraft {
    pub repo: PathBuf,
    pub name: String,
    pub base_branch: String,
    pub detected: Detected,
    pub probed: Vec<ProbedAdapter>,
    pub proposed: ProposedRunners,
}

/// What a person answered for one proposed runner: the adapter it runs on, and the
/// model it runs, none where the line was left empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerAnswer {
    pub runner: RunnerName,
    pub adapter: AdapterId,
    pub model: Option<ModelName>,
}

impl ProjectDraft {
    /// The adapters this machine answered for, in the order they were probed: what a
    /// runner's adapter is picked among, the first standing where none is picked. A
    /// project whose machine answers for none is set up with every runner proposed.
    pub fn answering(&self) -> Vec<&AdapterId> {
        self.probed
            .iter()
            .filter(|adapter| adapter.healthy)
            .map(|adapter| &adapter.id)
            .collect()
    }

    /// The setup a person answers this draft with: the `name` and `base_branch` typed,
    /// each left empty keeping what was detected; and each of `runners` declared on its
    /// adapter where it has a model, staying proposed where it has none.
    pub fn answered(
        &self,
        name: &str,
        base_branch: &str,
        runners: Vec<RunnerAnswer>,
    ) -> ProjectSetup {
        let kept = |typed: &str, detected: &str| match typed.is_empty() {
            true => detected.to_string(),
            false => typed.to_string(),
        };
        let declared: BTreeMap<RunnerName, RunnerCandidate> = runners
            .into_iter()
            .filter_map(|answer| {
                let model = answer.model?;
                let candidate = RunnerCandidate {
                    adapter: answer.adapter,
                    model,
                    agent: None,
                    effort: None,
                };
                Some((answer.runner, candidate))
            })
            .collect();
        ProjectSetup {
            name: kept(name, &self.name),
            base_branch: kept(base_branch, &self.base_branch),
            runners: match declared.is_empty() {
                true => RunnerDeclaration::Proposed,
                false => RunnerDeclaration::Declared(declared),
            },
        }
    }

    /// What setting the project up writes, by its path in the repository.
    pub fn writes(&self) -> Vec<String> {
        vec![
            CONFIG_WRITTEN.to_string(),
            GITIGNORE.to_string(),
            mechanism_skill_written(),
        ]
    }
}

/// The project's config, as setting it up writes it, by its path in the repository.
const CONFIG_WRITTEN: &str = ".yunta/config.yaml";

/// The ignore file setting a project up adds Yunta's entries to.
const GITIGNORE: &str = ".gitignore";

/// The mechanism skill setting a project up writes, by its path in the repository.
fn mechanism_skill_written() -> String {
    format!("{MECHANISM_SKILL_DIR}/SKILL.md")
}

/// What `init` wrote, and what it left as it was, with what it detected, the
/// adapters this machine answered for, the runners declared and those proposed
/// and left undeclared.
#[derive(Debug, Clone)]
pub struct InitializedProject {
    pub wrote: Vec<String>,
    pub skipped: Vec<String>,
    pub detected: Detected,
    pub probed: Vec<ProbedAdapter>,
    pub declared: BTreeMap<RunnerName, RunnerCandidate>,
    pub undeclared: Vec<RunnerName>,
}

impl DetectedProject {
    /// Writes the project as `setup` says: its config, from what the repository
    /// runs, the adapters this machine answers for and the runners as declared,
    /// parsed back before it is written; the ignore entries `init` adds once; and
    /// the mechanism skill, written again only under `force`.
    pub async fn write(self, setup: ProjectSetup) -> Result<InitializedProject, Error> {
        let draft = self.draft;
        let config_yaml = config_yaml(&setup, &draft.probed, &draft.detected, &draft.proposed);
        let repo = &self.workspace.dir;
        let (wrote, skipped) = write_project(repo, config_yaml, self.force).await?;
        let declared = match setup.runners {
            RunnerDeclaration::Proposed => BTreeMap::new(),
            RunnerDeclaration::Declared(declared) => declared,
        };
        let undeclared = draft
            .proposed
            .roles
            .into_iter()
            .filter(|role| !declared.contains_key(role))
            .collect();
        Ok(InitializedProject {
            wrote,
            skipped,
            detected: draft.detected,
            probed: draft.probed,
            declared,
            undeclared,
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
    let config_path = crate::project::repository_layer(repo);
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
    wrote.push(CONFIG_WRITTEN.to_string());

    match write_gitignore(repo) {
        Ok(true) => wrote.push(GITIGNORE.to_string()),
        Ok(false) => skipped.push(format!("{GITIGNORE} (already has yunta entries)")),
        Err(source) => return Err(unwritten("update", &".gitignore", source)),
    }

    match skill::write_mechanism_skill(repo, force) {
        Ok(true) => wrote.push(mechanism_skill_written()),
        Ok(false) => skipped.push(format!(
            "{} (already exists, pass --force to rewrite)",
            mechanism_skill_written()
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
