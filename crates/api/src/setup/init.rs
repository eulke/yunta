//! See [`super`]. A repository prepared for Yunta once: its ecosystem, test
//! command, base branch and the adapter CLIs this machine answers for are
//! detected, and the runner roles its workflows name proposed, with those of a
//! pack fetched to install beside it where there is one
//! (`DetectedProject::proposing`); then `.yunta/config.yaml`, defensive
//! `.gitignore` entries and the mechanism skill are written. Every value is the
//! best one detected; a caller that asks a person hands in what they answered
//! instead, the runners included.
//!
//! A role is declared with every candidate the machine answers for: the
//! adapter a person picked first, with the model they typed for it or none,
//! which leaves it to the adapter's CLI; then every other adapter that
//! answered, in the order they were probed, each with no model. A role nobody
//! answered for stands on the first that answered, so `init` unasked and every
//! question left write the same. Every adapter is a candidate so a run on any
//! of them finds the role there: a run pinned to an adapter takes the role's
//! candidate on it, and a binary that builds no adapter of the first takes the
//! next. Where no adapter answers, the roles stay proposed, commented.
//!
//! A repository whose workflows name no runner is proposed the one role that
//! stands for whatever a node naming none runs on (`ProposedRunners::StandIn`):
//! it is the default runner, `defaults.runner`, as well as declared, so a
//! workflow added afterwards runs on it. Where the workflows name their runners,
//! the default is the first, written only where a node names none. The config's
//! YAML is `config_yaml`'s, and what the workflows propose is `proposed`'s.

mod config_yaml;
mod proposed;

use std::path::{Path, PathBuf};

use yunta_core::refusal::SetupRefusal;
use yunta_core::step::NextStep;
use yunta_core::{
    AdapterId, Detected, ModelName, ProbedAdapter, RunnerCandidate, RunnerName, Workflow,
};
use yunta_engine::git::Cause;
use yunta_engine::process::Supervision;
use yunta_render::command;
use yunta_render::page::{Initialized, WrittenRunners};
use yunta_render::refusal::Refused;

use super::skill::{self, MECHANISM_SKILL_DIR};
use super::SetupUnwritten;
use crate::{Error, Failure, FetchedPack, Stop, Workspace};
use config_yaml::config_yaml;
pub use proposed::ProposedRunners;

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
        let draft = ProjectDraft {
            repo: self.dir.clone(),
            name,
            base_branch,
            detected,
            probed,
            proposed: proposed(self.catalog_workflows()),
        };
        Ok(DetectedProject {
            workspace: self.clone(),
            draft,
            force,
        })
    }
}

/// What a project is set up as: the name and base branch it is written under, how its
/// runners are written, and the proposal they were answered from, which names the roles
/// and the default runner written, as [`ProjectDraft::answered`] alone makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSetup {
    pub name: String,
    pub base_branch: String,
    runners: RunnerDeclaration,
    proposed: ProposedRunners,
}

impl ProjectSetup {
    /// How its runners are written.
    pub fn runners(&self) -> &RunnerDeclaration {
        &self.runners
    }

    /// The proposal its runners were answered from: the roles a commented setup writes,
    /// and the default runner every setup writes.
    pub fn proposed(&self) -> &ProposedRunners {
        &self.proposed
    }
}

/// How setting a project up writes its runners: every role proposed, either declared
/// or, where no adapter answers on this machine, commented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerDeclaration {
    /// Commented out, each proposed role on the first adapter this build knows, its
    /// model left to the adapter: what setting a project up writes where no adapter
    /// answers on this machine.
    Proposed,
    /// Every proposed role, in the order proposed, on its candidates.
    Declared(Vec<DeclaredRunner>),
}

/// A role declared, on its candidates in the order a run tries them, as the
/// configuration's list for the runner holds them: the adapter picked first, with the
/// model typed for it where one was, then every other adapter that answered, each with
/// no model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredRunner {
    pub runner: RunnerName,
    pub candidates: Vec<RunnerCandidate>,
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
/// model it runs, none where the line was left empty, which leaves it to the adapter.
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
    /// each left empty keeping what was detected; and every role proposed declared on its
    /// candidates, as its answer among `runners` picks the first or as the machine does
    /// where none answers for it; every role proposed, commented, where no adapter
    /// answers on this machine. It keeps the proposal it was answered from, which its
    /// write takes the default runner from.
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
        let answering = self.answering();
        let runners = match answering.first() {
            Some(first) => RunnerDeclaration::Declared(
                (self.proposed.roles().iter())
                    .map(|runner| {
                        let answer = runners.iter().find(|answer| &answer.runner == runner);
                        DeclaredRunner {
                            runner: runner.clone(),
                            candidates: candidates(answer, first, &answering),
                        }
                    })
                    .collect(),
            ),
            None => RunnerDeclaration::Proposed,
        };
        ProjectSetup {
            name: kept(name, &self.name),
            base_branch: kept(base_branch, &self.base_branch),
            runners,
            proposed: self.proposed.clone(),
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

/// The candidates a role answered as `answer` says is declared with, in the order a run
/// tries them: the adapter picked, with the model typed for it, or `first` of those that
/// `answering` where nothing was picked; then every other adapter answering, in the order
/// they were probed, each with no model.
fn candidates(
    answer: Option<&RunnerAnswer>,
    first: &AdapterId,
    answering: &[&AdapterId],
) -> Vec<RunnerCandidate> {
    let on = |adapter: &AdapterId, model: Option<ModelName>| RunnerCandidate {
        adapter: adapter.clone(),
        model,
        agent: None,
        effort: None,
    };
    let first = match answer {
        Some(answer) => on(&answer.adapter, answer.model.clone()),
        None => on(first, None),
    };
    let mut candidates: Vec<RunnerCandidate> = (answering.iter())
        .filter(|adapter| ***adapter != first.adapter)
        .map(|adapter| on(adapter, None))
        .collect();
    candidates.insert(0, first);
    candidates
}

/// What `init` wrote, and what it left as it was, with what it detected, the
/// adapters this machine answered for, and the runners as it wrote them beside the
/// proposal they came from, which names the default runner.
#[derive(Debug, Clone)]
pub struct InitializedProject {
    pub wrote: Vec<String>,
    pub skipped: Vec<String>,
    pub detected: Detected,
    pub probed: Vec<ProbedAdapter>,
    pub runners: RunnerDeclaration,
    pub proposed: ProposedRunners,
}

impl InitializedProject {
    /// The page `init` ends with, for the repository at `repo`.
    pub fn page<'a>(&'a self, repo: &'a Path) -> Initialized<'a> {
        let runners = match &self.runners {
            RunnerDeclaration::Declared(declared) => WrittenRunners::Declared(
                (declared.iter())
                    .map(|declared| (&declared.runner, declared.candidates.as_slice()))
                    .collect(),
            ),
            RunnerDeclaration::Proposed => WrittenRunners::Proposed(self.proposed.roles()),
        };
        Initialized {
            repo,
            wrote: &self.wrote,
            skipped: &self.skipped,
            detected: &self.detected,
            probed: &self.probed,
            runners,
            default: self.proposed.default_runner(),
        }
    }
}

impl DetectedProject {
    /// This project proposing the roles it would once `pack`, fetched, is installed:
    /// those of the repository's workflows and the pack's, in the order the catalog
    /// lists them then, so `init` after the pack's install proposes the same. Nothing
    /// is written.
    pub fn proposing(&mut self, pack: &FetchedPack) {
        self.draft.proposed = pack.proposed();
    }

    /// Writes the project as `setup` says: its config, from what the repository
    /// runs, the adapters this machine answers for and the runners as declared, with
    /// the default runner of the proposal `setup` was answered from, parsed back
    /// before it is written; the ignore entries `init` adds once; and the mechanism
    /// skill, written again only under `force`. Nothing of the draft's own proposal
    /// is read, so a proposal worked out again since the answer moves nothing.
    pub async fn write(self, setup: ProjectSetup) -> Result<InitializedProject, Error> {
        let draft = self.draft;
        let config_yaml = config_yaml(&setup, &draft.probed, &draft.detected);
        let repo = &self.workspace.dir;
        let (wrote, skipped) = write_project(repo, config_yaml, self.force).await?;
        Ok(InitializedProject {
            wrote,
            skipped,
            detected: draft.detected,
            probed: draft.probed,
            runners: setup.runners,
            proposed: setup.proposed,
        })
    }
}

impl FetchedPack {
    /// The roles a setup of the repository it installs into proposes once it is installed:
    /// those of the repository's workflows and its own, in the order the catalog lists them
    /// then. Read from the copy fetched and the repository alone, so it needs nothing a
    /// detection finds: what a setup shows while the pack is still to be confirmed, and a
    /// setup written without it writes none of. Nothing is written.
    pub fn proposed(&self) -> ProposedRunners {
        proposed(self.installs_into().catalog_workflows_installing(self))
    }
}

/// The roles the catalog's `workflows` propose, in the order it lists them.
fn proposed(workflows: Vec<(String, Workflow)>) -> ProposedRunners {
    let workflows: Vec<Workflow> = workflows
        .into_iter()
        .map(|(_, workflow)| workflow)
        .collect();
    ProposedRunners::of(&workflows)
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
    format!(
        "# added by `{}`",
        command::line(&NextStep::Init { interactive: false })
    )
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
