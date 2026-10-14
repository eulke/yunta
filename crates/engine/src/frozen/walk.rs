//! The freeze: every input a run's nodes interpret, read from the tree the
//! run is born in and kept by content.
//!
//! The walk reaches every node of the workflow and of each child it names
//! through `use:`, whatever a given mode or optional key leaves out: the
//! mode is chosen when a run starts, and the names it would read must be
//! frozen whichever one is chosen. A name that resolves to nothing, or to an
//! agent file no node could read as a definition, refuses the run at birth,
//! where it is written.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use yunta_core::{
    AgentName, ConfigLayer, ContentHash, ContextSpec, ExecutorName, Frozen, FrozenWorkflow,
    KnowledgeLayer, Node, NodeKind, RunnerCandidate, SkillName, Workflow,
};

use super::layers::layer_files;
use super::prompt::{read_prompts, PromptRoot};
use super::provenance::pack_provenance;
use super::read::{executor_path, read_file, read_file_text, registration, workflow_dir};
use super::{knowledge_key, FrozenError};
use crate::agents::{self, AgentsError};
use crate::catalog::{self, DeclaredPath, Named};
use crate::skills;

/// Freezes what a run born in `repo` from `workflow` under `config` will
/// interpret: its agents, skills, executors, child workflows and knowledge.
/// `user_knowledge` is the user layer's directory, when there is one.
///
/// `workflow` is the graph the run is built from, fan-out expanded. The
/// bytes behind the frozen set come back with it, each under the hash it
/// is named by.
pub(crate) async fn freeze(
    repo: &Path,
    workflow: &Workflow,
    config: &ConfigLayer,
    user_knowledge: Option<&Path>,
) -> Result<(Frozen, Vec<(ContentHash, Vec<u8>)>), FrozenError> {
    let mut reach = Reach::default();
    reach.visit(workflow, config);
    while let Some(use_name) = reach.uses.pop() {
        if reach.children.contains_key(&use_name) {
            continue;
        }
        let (child, child_workflow) = read_child(repo, &use_name).await?;
        reach.children.insert(use_name, child);
        reach.visit(&child_workflow, config);
    }
    let mut blobs = Blobs::default();
    let frozen = Frozen {
        agents: freeze_agents(repo, &reach.agents, &mut blobs).await?,
        skills: freeze_skills(repo, config, &reach.skills, &mut blobs).await?,
        executors: freeze_executors(repo, config, &reach.executors, &mut blobs).await?,
        workflows: reach.children,
        knowledge: freeze_knowledge(repo, user_knowledge, &reach.layers, &mut blobs).await?,
    };
    Ok((frozen, blobs.into_pairs()))
}

/// What the walk has seen so far: the names the nodes it reached interpret,
/// the child workflows they name, and the children read so far.
#[derive(Default)]
struct Reach {
    agents: BTreeSet<AgentName>,
    skills: BTreeSet<SkillName>,
    executors: BTreeSet<ExecutorName>,
    layers: Vec<KnowledgeLayer>,
    uses: Vec<String>,
    children: BTreeMap<String, FrozenWorkflow>,
}

impl Reach {
    /// Records what each node of `workflow` interprets, and queues the
    /// child workflows it names.
    fn visit(&mut self, workflow: &Workflow, config: &ConfigLayer) {
        for node in workflow.iter_nodes() {
            match &node.kind {
                _ if node.opens_session() => self.session(workflow, config, node),
                NodeKind::Executor { executor, .. } => {
                    self.executors.insert(executor.clone());
                }
                NodeKind::Workflow { r#use, .. } => self.uses.push(r#use.clone()),
                _ => {}
            }
        }
    }

    /// What a node that opens sessions interprets: the agent its sessions
    /// run as, the skills they mount, and the knowledge layers its context
    /// reads.
    fn session(&mut self, workflow: &Workflow, config: &ConfigLayer, node: &Node) {
        match &node.agent {
            // The node's own agent overrides the candidate's, so only the
            // candidates' agents are read when the node names none.
            Some(agent) => {
                self.agents.insert(agent.clone());
            }
            None => {
                for agent in
                    candidates(config, node).filter_map(|candidate| candidate.agent.clone())
                {
                    self.agents.insert(agent);
                }
            }
        }
        self.skills
            .extend(skills::skill_names(config, workflow, node));
        for spec in &node.context {
            if let ContextSpec::Knowledge { knowledge } = spec {
                for layer in knowledge.layers_read() {
                    if !self.layers.contains(&layer) {
                        self.layers.push(layer);
                    }
                }
            }
        }
    }
}

/// The candidates of the runner a session node names: its own runner, else
/// the defaults'.
fn candidates<'a>(
    config: &'a ConfigLayer,
    node: &'a Node,
) -> impl Iterator<Item = &'a RunnerCandidate> {
    let runners = config.runners.as_ref();
    config
        .runner_role(node)
        .and_then(|role| runners?.get(role))
        .into_iter()
        .flatten()
}

/// The bytes a freeze holds, each under the hash it is named by.
#[derive(Default)]
struct Blobs(BTreeMap<ContentHash, Vec<u8>>);

impl Blobs {
    /// Holds `bytes` and answers with the hash they are named by.
    fn keep(&mut self, bytes: Vec<u8>) -> ContentHash {
        let hash = ContentHash::sha256(&bytes);
        self.0.entry(hash.clone()).or_insert(bytes);
        hash
    }

    fn into_pairs(self) -> Vec<(ContentHash, Vec<u8>)> {
        self.0.into_iter().collect()
    }
}

/// The child workflow `use_name` names in the tree the run is born in, read
/// as the child's own run would read it, with the text of each prompt file
/// its nodes read and the pack it came from, as the tree stands now.
async fn read_child(
    repo: &Path,
    use_name: &str,
) -> Result<(FrozenWorkflow, Workflow), FrozenError> {
    let (path, declared) =
        match catalog::name_workflow(repo, use_name).map_err(FrozenError::unresolved)? {
            Named::Repo(path) => (path, None),
            Named::Pack { declared, .. } => (contained(&declared)?, Some(declared)),
        };
    let dir = workflow_dir(repo, &path);
    let root = match declared {
        Some(declared) => PromptRoot::Pack {
            pack: declared.pack,
            pack_dir: declared.pack_dir,
            dir: dir.clone(),
        },
        None => PromptRoot::Repo(dir.clone()),
    };
    let text = read_file_text(&path).await?;
    let workflow =
        yunta_core::workflow::read::read(&text, &path).map_err(FrozenError::unresolved)?;
    let prompts = read_prompts(&workflow, &root).await?;
    let (pack, provenance_gaps) = pack_provenance(repo, &dir).await;
    Ok((
        FrozenWorkflow {
            text,
            prompts,
            pack,
            provenance_gaps,
        },
        workflow,
    ))
}

async fn freeze_agents(
    repo: &Path,
    names: &BTreeSet<AgentName>,
    blobs: &mut Blobs,
) -> Result<BTreeMap<AgentName, ContentHash>, FrozenError> {
    let mut frozen = BTreeMap::new();
    for name in names {
        let path = agent_path(repo, name)?;
        let bytes = read_file(&path).await?;
        check_agent(name, &path, &bytes)?;
        frozen.insert(name.clone(), blobs.keep(bytes));
    }
    Ok(frozen)
}

/// The path of the definition `name` names in the tree the run is born in:
/// the project's own file, else the one pack file that declares it.
fn agent_path(repo: &Path, name: &AgentName) -> Result<PathBuf, FrozenError> {
    match agents::locate(repo, name).map_err(FrozenError::unresolved)? {
        agents::Location::Project(path) => Ok(path),
        agents::Location::Pack(declared) => contained(&declared),
    }
}

/// Refuses the bytes of an agent's file unless a node could read them as the
/// definition of `name`: they must be UTF-8, and `agents::definition` must
/// parse them. `path` names the file in the refusal.
fn check_agent(name: &AgentName, path: &Path, bytes: &[u8]) -> Result<(), FrozenError> {
    let text = std::str::from_utf8(bytes).map_err(|source| {
        FrozenError::unresolved(AgentsError::Unreadable {
            name: name.clone(),
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
        })
    })?;
    agents::definition(name, text)
        .map(|_| ())
        .map_err(FrozenError::unresolved)
}

async fn freeze_skills(
    repo: &Path,
    config: &ConfigLayer,
    names: &BTreeSet<SkillName>,
    blobs: &mut Blobs,
) -> Result<BTreeMap<SkillName, BTreeMap<PathBuf, ContentHash>>, FrozenError> {
    let mut frozen = BTreeMap::new();
    for name in names {
        let dir = skill_root(repo, config, name)?;
        let mut files = BTreeMap::new();
        for file in skill_files(&dir).await? {
            let bytes = read_file(&dir.join(&file)).await?;
            files.insert(file, blobs.keep(bytes));
        }
        frozen.insert(name.clone(), files);
    }
    Ok(frozen)
}

/// The directory a skill name names in the tree the run is born in.
fn skill_root(repo: &Path, config: &ConfigLayer, name: &SkillName) -> Result<PathBuf, FrozenError> {
    match skills::locate(config, repo, name).map_err(FrozenError::unresolved)? {
        skills::Location::Dir(dir) => Ok(dir),
        skills::Location::Pack(declared) => contained(&declared),
    }
}

/// The files under a skill directory, relative to it. A symlink to a file
/// counts as that file, so the copy holds the bytes a session would read
/// through the link. A symlink to a directory is not followed, so no link
/// can loop.
async fn skill_files(dir: &Path) -> Result<Vec<PathBuf>, FrozenError> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let here = dir.join(&relative);
        let io = |source: std::io::Error| FrozenError::Io {
            context: format!("list `{}`", here.display()),
            source,
        };
        let mut listing = tokio::fs::read_dir(&here).await.map_err(io)?;
        while let Some(entry) = listing.next_entry().await.map_err(io)? {
            let path = relative.join(entry.file_name());
            let kind = entry.file_type().await.map_err(io)?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() || (kind.is_symlink() && is_file(&entry.path()).await) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Whether `path` is a file once its symlinks are followed.
async fn is_file(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .is_ok_and(|meta| meta.is_file())
}

async fn freeze_executors(
    repo: &Path,
    config: &ConfigLayer,
    names: &BTreeSet<ExecutorName>,
    blobs: &mut Blobs,
) -> Result<BTreeMap<ExecutorName, ContentHash>, FrozenError> {
    let mut frozen = BTreeMap::new();
    for name in names {
        // A name the config does not register has no binary to freeze: its
        // node fails on the missing registration when it runs.
        let Some(found) = registration(config, name) else {
            continue;
        };
        let bytes = read_file(&executor_path(repo, found)).await?;
        frozen.insert(name.clone(), blobs.keep(bytes));
    }
    Ok(frozen)
}

async fn freeze_knowledge(
    repo: &Path,
    user: Option<&Path>,
    layers: &[KnowledgeLayer],
    blobs: &mut Blobs,
) -> Result<BTreeMap<String, ContentHash>, FrozenError> {
    let mut frozen = BTreeMap::new();
    for layer in layers {
        for file in layer_files(*layer, repo, user, contained).await? {
            let key = knowledge_key(*layer, file.pack.as_deref(), &file.relative);
            let bytes = read_file(&file.path).await?;
            frozen.insert(key, blobs.keep(bytes));
        }
    }
    Ok(frozen)
}

/// The path a pack declares, joined under the pack's directory, when it
/// stays inside the pack. A path that leaves it is refused, naming the pack
/// and the path.
fn contained(declared: &DeclaredPath) -> Result<PathBuf, FrozenError> {
    declared.contained().ok_or_else(|| FrozenError::Escapes {
        pack: declared.pack.clone(),
        path: declared.declared.clone(),
    })
}

#[cfg(test)]
mod tests;
