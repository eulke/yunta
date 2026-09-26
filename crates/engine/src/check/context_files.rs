//! See [`super`]. Whether the files a `files:` context source names are in
//! the tree a run would start from — the second entry point that reads
//! the repository, beside [`super::check_workflow_refs`], so `check` keeps
//! reading none.
//!
//! A warning and never a refusal: a node that runs earlier may write the
//! file, and only the run knows. What this answers is the question a
//! person can still act on for free — before the first token, not after
//! the nodes ahead of the reader have spent theirs.

use std::path::{Path, PathBuf};

use yunta_core::template::template_variables;
use yunta_core::{CommitSha, ContextSpec, Isolation};

use super::*;
use crate::process::Supervision;

/// How a `files:` path that the run would not find is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingContextFile {
    /// Neither in the commit the run starts from nor on disk.
    Nowhere,
    /// On disk in the person's checkout, and not committed.
    Uncommitted,
    /// On disk in the person's checkout, and ignored by git.
    Ignored,
}

/// The tree a run would read its `files:` from.
#[derive(Clone, Copy)]
pub struct RunTreeOrigin<'a> {
    /// Where the run is started — any directory of the repository.
    pub checkout: &'a Path,
    /// `worktree` reads the commit below; `none` reads the checkout
    /// itself.
    pub isolation: Isolation,
    /// The commit an isolated run's tree starts from.
    pub base: &'a CommitSha,
}

/// Every literal `files:` path a node of `workflow` would read and the run
/// would not find, among the nodes `mode_nodes` includes (every node when
/// `None`). A path built from a template is only known once the run
/// renders it, and is left to the run; so is anything git cannot answer
/// here.
pub async fn check_context_files(
    workflow: &Workflow,
    mode_nodes: Option<&HashSet<NodeId>>,
    origin: RunTreeOrigin<'_>,
    supervision: Supervision<'_>,
) -> Vec<CheckWarning> {
    let mut read = Vec::new();
    for node in workflow
        .nodes
        .iter()
        .filter(|node| mode_nodes.is_none_or(|set| set.contains(&node.id)))
    {
        literal_files(node, &mut read);
    }
    if read.is_empty() {
        return Vec::new();
    }
    let root = repository_root(origin.checkout, supervision).await;
    let mut warnings = Vec::new();
    for (node, path) in read {
        if let Some(missing) = missing(&path, root.as_deref(), origin, supervision).await {
            let base = (origin.isolation == Isolation::Worktree && !Path::new(&path).is_absolute())
                .then(|| origin.base.abbreviated().to_string());
            warnings.push(CheckWarning::ContextFileMissing {
                node,
                path,
                base,
                missing,
            });
        }
    }
    warnings
}

/// `(node, path)` for every literal `files:` entry of `node` and of the
/// children a `parallel` group runs, each pair once.
fn literal_files(node: &Node, read: &mut Vec<(NodeId, String)>) {
    for source in &node.context {
        let ContextSpec::Files { files } = source else {
            continue;
        };
        for path in files {
            let literal = template_variables(path).is_ok_and(|vars| vars.is_empty());
            let entry = (node.id.clone(), path.clone());
            if literal && !read.contains(&entry) {
                read.push(entry);
            }
        }
    }
    if let NodeKind::Parallel { nodes, .. } = &node.kind {
        for child in nodes {
            literal_files(child, read);
        }
    }
}

/// The top of the working tree `checkout` belongs to, which is what a
/// run's relative paths are relative to.
async fn repository_root(checkout: &Path, supervision: Supervision<'_>) -> Option<PathBuf> {
    crate::git::output(checkout, &["rev-parse", "--show-toplevel"], supervision)
        .await
        .ok()
        .map(|printed| PathBuf::from(printed.trim()))
}

/// How `path` is missing from the tree the run would read, or `None` when
/// it is there — or when git cannot say, which is the run's to find out.
async fn missing(
    path: &str,
    root: Option<&Path>,
    origin: RunTreeOrigin<'_>,
    supervision: Supervision<'_>,
) -> Option<MissingContextFile> {
    if Path::new(path).is_absolute() {
        return (!Path::new(path).exists()).then_some(MissingContextFile::Nowhere);
    }
    let root = root?;
    let on_disk = root.join(path).exists();
    if origin.isolation == Isolation::None {
        return (!on_disk).then_some(MissingContextFile::Nowhere);
    }
    let spec = format!("{}:{path}", origin.base.as_str());
    let committed = crate::git::success(root, &["cat-file", "-e", spec.as_str()], supervision)
        .await
        .ok()?;
    if committed {
        return None;
    }
    if !on_disk {
        return Some(MissingContextFile::Nowhere);
    }
    let ignored = crate::git::success(root, &["check-ignore", "-q", path], supervision)
        .await
        .ok()?;
    Some(if ignored {
        MissingContextFile::Ignored
    } else {
        MissingContextFile::Uncommitted
    })
}
