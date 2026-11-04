//! What a run changed, as git counts it: the count a gate, a pull request and a
//! run's close stand on.

use serde::{Deserialize, Serialize};

/// What the run's change stands at, counted by git from the run's base
/// commit to its newest tree: how many files it touches and how many lines
/// it adds and removes, a binary file counting as a file with no lines. It
/// is counted when a gate asks a person, before a pull request opens, and
/// when the engine finishes the run (`done`, `failed` or `promoted`), never
/// once per node: the event names the gate or the pull request node it was
/// counted for, and no node at the run's end. A person's close of a stopped
/// run (`yunta close`) records no count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChangesCountedPayload {
    /// The tree the change was counted to.
    pub tree: crate::TreeId,
    /// How many files the change touches: a renamed file once, binary
    /// files included.
    pub files: u32,
    /// How many lines the change adds.
    pub added: u64,
    /// How many lines the change removes.
    pub removed: u64,
    /// The files the change moves most, the largest first, as many as a page
    /// names. Absent in a log older than the field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub largest: Vec<FileChange>,
}

/// One file of a change, as git counts it: a renamed file by its new path, and a
/// binary file with no lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FileChange {
    /// The file, relative to the repository. A byte of its name that is not
    /// text reads as the replacement character.
    pub path: String,
    /// How many lines the change adds to it.
    pub added: u64,
    /// How many lines the change removes from it.
    pub removed: u64,
}
