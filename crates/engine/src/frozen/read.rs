//! The door a node reads its inputs through.
//!
//! A run born with a frozen set answers every read from that set: each
//! file is verified against the hash it was born with before it is
//! returned. A run that predates frozen sets reads the checkout it has, as
//! it always did, through the same lookups the freeze makes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use yunta_core::agent::AgentDefinition;
use yunta_core::{
    AgentName, ConfigKey, ConfigLayer, ContentHash, Env, ExecutorName, ExecutorRegistration,
    Frozen, KnowledgeLayer, Manifest, SkillName,
};

use super::layers::layer_files;
use super::store;
use super::{
    agent_input, executor_input, knowledge_input, skill_input, split_knowledge_key, workflow_input,
    FrozenError,
};
use crate::agents::{self, AgentsError};
use crate::artifacts::store::ObjectStore;
use crate::catalog;
use crate::skills;

/// Where a checkout's workflows sit when a workflow's path has no parent.
const WORKFLOWS_DIR: &str = ".yunta/workflows";

/// Where a run's inputs come from: its frozen set when its manifest has
/// one, the checkout when it does not.
pub(crate) enum Source<'a> {
    /// The run's own copies and objects; `run_dir` holds both.
    Frozen {
        frozen: &'a Frozen,
        run_dir: &'a Path,
    },
    /// The checkout as it stands now, read through the same resolution the
    /// freeze uses. `config` is the run's own configuration, which names
    /// the skills paths and the executors.
    Checkout {
        worktree: &'a Path,
        config: &'a ConfigLayer,
        ambient: Option<&'a Env>,
    },
}

/// One file of a knowledge layer.
pub(crate) struct KnowledgeFile {
    /// The pack the file came from; `None` for the repo and user layers.
    pub(crate) pack: Option<String>,
    /// The file's name, the key a layer's files merge by.
    pub(crate) name: OsString,
    /// The file's path within the root its layer reads it from (the layer's
    /// directory, or the root a pack declares), with `/` separators.
    pub(crate) relative: String,
    pub(crate) bytes: Vec<u8>,
}

/// A child workflow as its run is born with it: the copy the parent run
/// froze, or the checkout's file.
pub(crate) enum ChildWorkflow<'a> {
    /// The text the parent froze, with the frozen set it came with.
    Frozen { text: String, parent: &'a Frozen },
    /// The checkout's text, with the directory its prompt files resolve beside.
    Checkout { text: String, dir: PathBuf },
}

impl ChildWorkflow<'_> {
    /// The workflow's text, as its frozen copy or its file holds it.
    pub(crate) fn text(&self) -> &str {
        match self {
            ChildWorkflow::Frozen { text, .. } | ChildWorkflow::Checkout { text, .. } => text,
        }
    }
}

impl<'a> Source<'a> {
    /// The source a run's nodes read through: its frozen set when its
    /// manifest has one. `ambient` is the environment the run executes in,
    /// which names the user layer of a checkout read.
    pub(crate) fn of(
        manifest: &'a Manifest,
        run_dir: &'a Path,
        worktree: &'a Path,
        ambient: Option<&'a Env>,
    ) -> Source<'a> {
        match &manifest.frozen {
            Some(frozen) => Source::Frozen { frozen, run_dir },
            None => Source::Checkout {
                worktree,
                config: &manifest.config,
                ambient,
            },
        }
    }

    /// The definition `name` names. A definition whose file does not parse
    /// is the resolver's sentence, as a checkout read has always said it.
    pub(crate) async fn agent(&self, name: &AgentName) -> Result<AgentDefinition, FrozenError> {
        let text = match self {
            Source::Frozen { frozen, run_dir } => {
                let input = agent_input(name);
                let hash = frozen
                    .agents
                    .get(name)
                    .ok_or_else(|| FrozenError::Missing {
                        input: input.clone(),
                    })?;
                let bytes = store::object(run_dir, hash, &input).await?;
                // The bytes match their frozen hash, so bytes that are not UTF-8
                // are a definition that does not read, as a checkout read reports
                // them, and not a mismatch.
                String::from_utf8(bytes).map_err(|source| {
                    FrozenError::unresolved(AgentsError::Unreadable {
                        name: name.clone(),
                        path: ObjectStore::at(run_dir).path_of(hash),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
                    })
                })?
            }
            Source::Checkout { worktree, .. } => checkout_agent_text(worktree, name).await?,
        };
        agents::definition(name, &text).map_err(FrozenError::unresolved)
    }

    /// The directory a skill mounts from. A frozen copy must hold exactly the
    /// files the run froze, and each of them must hash to its frozen value,
    /// before it mounts.
    pub(crate) async fn skill_dir(&self, name: &SkillName) -> Result<PathBuf, FrozenError> {
        match self {
            Source::Frozen { frozen, run_dir } => {
                let input = format!("skills/{name}");
                let files = frozen
                    .skills
                    .get(name)
                    .ok_or_else(|| FrozenError::Missing {
                        input: input.clone(),
                    })?;
                let dir = store::skill_copy(run_dir, name);
                if !tokio::fs::metadata(&dir)
                    .await
                    .is_ok_and(|meta| meta.is_dir())
                {
                    return Err(FrozenError::Mismatch { input });
                }
                // Both lists are sorted, so equal sequences hold the same files.
                let held = store::copied_files(&dir, &input).await?;
                if !held.iter().eq(files.keys()) {
                    return Err(FrozenError::Mismatch { input });
                }
                for (file, hash) in files {
                    verify_copy(&dir.join(file), hash, &skill_input(name, file)).await?;
                }
                Ok(dir)
            }
            Source::Checkout {
                worktree, config, ..
            } => match skills::locate(config, worktree, name).map_err(FrozenError::unresolved)? {
                skills::Location::Dir(dir) => Ok(dir),
                skills::Location::Pack(declared) => Ok(declared.joined()),
            },
        }
    }

    /// The executor's binary. A frozen binary is verified against its hash
    /// before it runs.
    pub(crate) async fn executor(&self, name: &ExecutorName) -> Result<PathBuf, FrozenError> {
        match self {
            Source::Frozen { frozen, run_dir } => {
                let input = executor_input(name);
                let hash = frozen
                    .executors
                    .get(name)
                    .ok_or_else(|| FrozenError::Missing {
                        input: input.clone(),
                    })?;
                let path = store::executor_copy(run_dir, name);
                verify_copy(&path, hash, &input).await?;
                Ok(path)
            }
            Source::Checkout {
                worktree, config, ..
            } => {
                let found = registration(config, name).ok_or_else(|| {
                    FrozenError::Unresolved(
                        ConfigKey::Executor {
                            executor: name.clone(),
                        }
                        .to_string(),
                    )
                })?;
                Ok(executor_path(worktree, found))
            }
        }
    }

    /// The child workflow `use_name` names, as its run is born with it: the
    /// copy the parent froze, or the checkout's file, resolved once.
    pub(crate) async fn workflow(&self, use_name: &str) -> Result<ChildWorkflow<'_>, FrozenError> {
        match self {
            Source::Frozen { frozen, .. } => {
                let child = frozen
                    .workflows
                    .get(use_name)
                    .ok_or_else(|| FrozenError::Missing {
                        input: workflow_input(use_name),
                    })?;
                Ok(ChildWorkflow::Frozen {
                    text: child.text.clone(),
                    parent: frozen,
                })
            }
            Source::Checkout { worktree, .. } => {
                let resolved = catalog::resolve_workflow(worktree, use_name)
                    .map_err(FrozenError::unresolved)?;
                let text = read_file_text(&resolved.path).await?;
                Ok(ChildWorkflow::Checkout {
                    text,
                    dir: workflow_dir(worktree, &resolved.path),
                })
            }
        }
    }

    /// The files one knowledge layer holds.
    pub(crate) async fn knowledge(
        &self,
        layer: KnowledgeLayer,
    ) -> Result<Vec<KnowledgeFile>, FrozenError> {
        match self {
            Source::Frozen { frozen, run_dir } => frozen_knowledge(frozen, run_dir, layer).await,
            Source::Checkout {
                worktree, ambient, ..
            } => checkout_knowledge(worktree, *ambient, layer).await,
        }
    }
}

/// The checkout's text of `name`'s definition, the project's own file
/// first, then the one pack file that declares it.
async fn checkout_agent_text(worktree: &Path, name: &AgentName) -> Result<String, FrozenError> {
    let path = match agents::locate(worktree, name).map_err(FrozenError::unresolved)? {
        agents::Location::Project(path) => path,
        agents::Location::Pack(declared) => declared.joined(),
    };
    tokio::fs::read_to_string(&path).await.map_err(|source| {
        FrozenError::unresolved(AgentsError::Unreadable {
            name: name.clone(),
            path,
            source,
        })
    })
}

/// The frozen files of one layer, verified, in the order their keys sort.
async fn frozen_knowledge(
    frozen: &Frozen,
    run_dir: &Path,
    layer: KnowledgeLayer,
) -> Result<Vec<KnowledgeFile>, FrozenError> {
    let layer_key = layer.to_string();
    let mut files = Vec::new();
    for (key, hash) in &frozen.knowledge {
        let Some((key_layer, pack, relative)) = split_knowledge_key(key) else {
            return Err(FrozenError::UnreadableKey {
                input: knowledge_input(key),
            });
        };
        if key_layer != layer_key {
            continue;
        }
        let Some(name) = Path::new(relative).file_name() else {
            continue;
        };
        let bytes = store::object(run_dir, hash, &knowledge_input(key)).await?;
        files.push(KnowledgeFile {
            pack: pack.map(str::to_string),
            name: name.to_os_string(),
            relative: relative.to_string(),
            bytes,
        });
    }
    Ok(files)
}

/// The files one layer holds in the checkout right now, listed by the same
/// rule the freeze lists them by, and read as they stand.
async fn checkout_knowledge(
    worktree: &Path,
    ambient: Option<&Env>,
    layer: KnowledgeLayer,
) -> Result<Vec<KnowledgeFile>, FrozenError> {
    let user = ambient.and_then(yunta_core::user_knowledge_dir);
    let mut files = Vec::new();
    for file in layer_files(layer, worktree, user.as_deref(), |declared| {
        Ok(declared.joined())
    })
    .await?
    {
        let bytes = read_file(&file.path).await?;
        files.push(KnowledgeFile {
            pack: file.pack,
            name: file.name,
            relative: file.relative,
            bytes,
        });
    }
    Ok(files)
}

/// The bytes of a file, as an I/O failure names it.
pub(super) async fn read_file(path: &Path) -> Result<Vec<u8>, FrozenError> {
    tokio::fs::read(path)
        .await
        .map_err(|source| FrozenError::Io {
            context: format!("read `{}`", path.display()),
            source,
        })
}

/// The text of a file, as an I/O failure names it.
pub(super) async fn read_file_text(path: &Path) -> Result<String, FrozenError> {
    tokio::fs::read_to_string(path)
        .await
        .map_err(|source| FrozenError::Io {
            context: format!("read `{}`", path.display()),
            source,
        })
}

/// The executor registration `name` names in the run's configuration.
pub(crate) fn registration<'c>(
    config: &'c ConfigLayer,
    name: &ExecutorName,
) -> Option<&'c ExecutorRegistration> {
    config
        .skills
        .as_ref()?
        .executors
        .iter()
        .find(|executor| executor.name == *name)
}

/// Where an executor's binary sits: joined onto the checkout when relative.
pub(crate) fn executor_path(worktree: &Path, registration: &ExecutorRegistration) -> PathBuf {
    worktree.join(&registration.path)
}

/// The directory a workflow read from the checkout keeps its prompt files
/// in: the directory its own file sits in, else the repo's workflows
/// directory when the path has no parent.
pub(crate) fn workflow_dir(repo: &Path, file: &Path) -> PathBuf {
    file.parent()
        .map_or_else(|| repo.join(WORKFLOWS_DIR), Path::to_path_buf)
}

/// Checks the copy at `path` against `hash`: a copy that is gone or holds
/// other bytes no longer matches what the run was born with.
async fn verify_copy(path: &Path, hash: &ContentHash, input: &str) -> Result<(), FrozenError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(FrozenError::Mismatch {
                input: input.to_string(),
            })
        }
        Err(source) => {
            return Err(FrozenError::Io {
                context: format!("read `{}`", path.display()),
                source,
            })
        }
    };
    if ContentHash::sha256(&bytes) != *hash {
        return Err(FrozenError::Mismatch {
            input: input.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
