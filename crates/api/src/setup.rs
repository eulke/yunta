//! See [`crate`]. Setting a repository up for Yunta: `init`, which writes the
//! project's config from what the repository and this machine say, its
//! defensive ignore entries and the mechanism skill (`skill`); the pack a
//! setup offers to install, whose workflows' roles it proposes once fetched
//! (`pack`);
//! and a new workflow from a skeleton, judged as `check` judges a workflow
//! (`workflow`). All belong to the repository a workspace works in: `init`
//! is what makes a project, so none resolves one before it writes.

mod init;
mod pack;
mod skill;
mod workflow;

pub use init::{
    DeclaredRunner, DetectedProject, InitializedProject, ProjectDraft, ProjectSetup,
    ProposedRunners, RunnerAnswer, RunnerDeclaration,
};
pub use pack::SetupPack;
pub use workflow::{WorkflowStem, WrittenWorkflow};

/// A file or a directory `init` or `new` could not write: what it was doing,
/// and the disk's words.
#[derive(Debug, thiserror::Error)]
#[error("failed to {doing}: {source}")]
struct SetupUnwritten {
    doing: String,
    source: std::io::Error,
}
