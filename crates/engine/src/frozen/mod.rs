//! What a run's nodes read, frozen when the run is born.
//!
//! A run's agents, skills, executors, child workflows and knowledge are
//! read once, when the run is created, and kept by content in the run's
//! own object store. A node reads them through [`read::Source`]: a run
//! born with a frozen set answers from that set and verifies every byte
//! against the hash it was born with, and a run that predates frozen sets
//! reads the checkout it has, as it always did.
//!
//! [`walk`] freezes, [`store`] keeps and materializes, and [`read`] is
//! the door a node reads through. [`prompt`] reads a workflow's prompt
//! files from where its pack allows, [`layers`] lists the files of each
//! knowledge layer, [`listing`] lists a directory's files for every copy and
//! layer, and [`provenance`] says which pack a workflow came from.

pub(crate) mod layers;
pub(crate) mod listing;
pub(crate) mod prompt;
pub(crate) mod provenance;
pub(crate) mod read;
pub(crate) mod store;
pub(crate) mod walk;

pub(crate) use read::{KnowledgeFile, Source};

use std::path::{Path, PathBuf};

use yunta_core::{AgentName, ContentHash, ExecutorName, Frozen, KnowledgeLayer, NodeId, SkillName};

/// Why a run's frozen inputs cannot be frozen, or cannot be read back.
#[derive(Debug, thiserror::Error)]
pub enum FrozenError {
    /// A name that resolves to nothing, or to a definition that cannot be
    /// read. The message is the resolver's own sentence.
    #[error("{0}")]
    Unresolved(String),
    /// The run holds no bytes for an input its frozen set names.
    #[error("the run holds no bytes for `{input}`")]
    Missing { input: String },
    /// An input's bytes no longer hash to the value the run was born with, or
    /// a copy of it no longer holds exactly the files the run was born with.
    #[error("`{input}` no longer holds the content the run was born with")]
    Mismatch { input: String },
    /// A frozen knowledge key that names no layer this binary reads: the
    /// frozen set does not hold what the freeze wrote.
    #[error(
        "the run's frozen set holds `{input}`, which is not a knowledge key this binary reads"
    )]
    UnreadableKey { input: String },
    /// A path a pack declares, or a prompt file a pack workflow names, leaves
    /// the pack's own directory.
    #[error("pack `{pack}` declares `{path}`, which leaves the pack's own directory")]
    Escapes { pack: String, path: String },
    /// A prompt file a node reads cannot be read; the error names the node
    /// and the file.
    #[error("node `{node}` references prompt file `{path}` which cannot be read")]
    Prompt {
        node: NodeId,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Reading or writing a file, or an object of the run's store, failed.
    #[error("failed to {context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl FrozenError {
    /// A name that resolves to nothing, carried as the resolver's sentence.
    pub(crate) fn unresolved(error: impl std::fmt::Display) -> Self {
        FrozenError::Unresolved(error.to_string())
    }
}

/// The input an agent's frozen definition is reported as.
pub(crate) fn agent_input(name: &AgentName) -> String {
    format!("agents/{name}.md")
}

/// The input one file of a skill is reported as.
pub(crate) fn skill_input(name: &SkillName, file: &Path) -> String {
    format!("skills/{name}/{}", file.display())
}

/// The input an executor's frozen binary is reported as.
pub(crate) fn executor_input(name: &ExecutorName) -> String {
    format!("executors/{name}")
}

/// The input a child workflow's frozen text is reported as.
pub(crate) fn workflow_input(use_name: &str) -> String {
    format!("workflows/{use_name}")
}

/// The input a knowledge file is reported as: its frozen key, with the
/// separators spelled as path separators.
pub(crate) fn knowledge_input(key: &str) -> String {
    format!("knowledge/{}", key.replace(':', "/"))
}

/// The frozen key of a knowledge file: `<layer>:<relative path>`, or
/// `org:<pack>:<relative path>` for a file a pack declares. The layer is
/// spelled by its `Display`, the one spelling every reader shares.
pub(crate) fn knowledge_key(layer: KnowledgeLayer, pack: Option<&str>, relative: &str) -> String {
    match pack {
        Some(pack) => format!("{layer}:{pack}:{relative}"),
        None => format!("{layer}:{relative}"),
    }
}

/// The layer, the pack (org only) and the relative path a frozen key names:
/// the reading of [`knowledge_key`]. `None` for a key that names no such
/// file.
pub(crate) fn split_knowledge_key(key: &str) -> Option<(&str, Option<&str>, &str)> {
    let (layer, rest) = key.split_once(':')?;
    if layer == KnowledgeLayer::Org.to_string() {
        let (pack, relative) = rest.split_once(':')?;
        Some((layer, Some(pack), relative))
    } else {
        Some((layer, None, rest))
    }
}

/// Every hash a frozen set names, each with the input it belongs to, in a
/// fixed order.
pub(crate) fn named_inputs(frozen: &Frozen) -> Vec<(String, &ContentHash)> {
    let agents = frozen
        .agents
        .iter()
        .map(|(name, hash)| (agent_input(name), hash));
    let skills = frozen.skills.iter().flat_map(|(name, files)| {
        files
            .iter()
            .map(move |(file, hash)| (skill_input(name, file), hash))
    });
    let executors = frozen
        .executors
        .iter()
        .map(|(name, hash)| (executor_input(name), hash));
    let knowledge = frozen
        .knowledge
        .iter()
        .map(|(key, hash)| (knowledge_input(key), hash));
    agents
        .chain(skills)
        .chain(executors)
        .chain(knowledge)
        .collect()
}
