//! See [`super`]. Whether the files a `files:` context source names are in
//! the tree a run would start from — the second entry point that reads
//! the repository, beside [`super::check_workflow_refs`], so `check` keeps
//! reading none.
//!
//! A missing file nothing that runs before its reader can write stops
//! that node every time it runs, and is refused. One a node that runs
//! earlier might write is only the run's to know, and is a warning. Both
//! are said when a person can still act on them for free — before the
//! first token, not after the nodes ahead of the reader have spent
//! theirs.

use std::path::{Path, PathBuf};

use yunta_core::template::template_variables;
use yunta_core::{CommitSha, ContextSpec, Isolation};

use super::*;
use crate::git::{GitError, CHECK_IGNORE};
use crate::process::Supervision;

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

/// What the `files:` walk found: the paths no node could supply before
/// their reader, and the ones only a node that runs earlier could.
#[derive(Debug, Default)]
pub struct ContextFilesCheck {
    pub errors: Vec<CheckError>,
    pub warnings: Vec<Caution>,
}

/// Whether anything could still put a missing file where its reader
/// looks: what tells a refusal from a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reach {
    EarlierNodeMay,
    NothingEarlierCan,
}

/// Every literal `files:` path a node of `workflow` would read and the run
/// would not find, among the nodes `mode_nodes` includes (every node when
/// `None`). A path built from a template is only known once the run
/// renders it, and is left to the run. A git that cannot answer is an
/// error, not a finding.
pub async fn check_context_files(
    workflow: &Workflow,
    mode_nodes: Option<&HashSet<NodeId>>,
    origin: RunTreeOrigin<'_>,
    supervision: Supervision<'_>,
) -> Result<ContextFilesCheck, GitError> {
    let mut read = Vec::new();
    for node in workflow
        .nodes
        .iter()
        .filter(|node| mode_nodes.is_none_or(|set| set.contains(&node.id)))
    {
        literal_files(node, &mut read);
    }
    let mut found = ContextFilesCheck::default();
    if read.is_empty() {
        return Ok(found);
    }
    let root = repository_root(origin.checkout, supervision).await?;
    for (node, path) in read {
        if let Some(missing) = missing(&path, &root, origin, supervision).await? {
            let base = (origin.isolation == Isolation::Worktree && !Path::new(&path).is_absolute())
                .then(|| origin.base.abbreviated().to_string());
            match reach(workflow, &node, &path, mode_nodes) {
                Reach::EarlierNodeMay => found.warnings.push(Caution::ContextFileMissing {
                    node,
                    path,
                    base,
                    missing,
                }),
                Reach::NothingEarlierCan => found.errors.push(CheckError::ContextFileUnreachable {
                    node,
                    path,
                    base,
                    missing,
                }),
            }
        }
    }
    Ok(found)
}

/// Whether something that can run before `reader` might write `path`: a
/// `before` hook of its own or of its group, an ancestor in the mode, the
/// node its failure re-routes to and that node's ancestors (the reader
/// runs again after them), or a sibling in its `parallel` group.
fn reach(
    workflow: &Workflow,
    reader: &NodeId,
    path: &str,
    mode_nodes: Option<&HashSet<NodeId>>,
) -> Reach {
    let defaults = workflow
        .node_defaults
        .as_ref()
        .and_then(|defaults| defaults.hooks.as_ref());
    let Some((node, group)) = workflow
        .iter_nodes_with_group()
        .find(|(node, _)| node.id == *reader)
    else {
        return Reach::EarlierNodeMay;
    };
    let runs_before_hooks =
        |node: &Node| hooks_of(node, defaults).is_some_and(|hooks| !hooks.before.is_empty());
    if runs_before_hooks(node) || group.is_some_and(runs_before_hooks) {
        return Reach::EarlierNodeMay;
    }

    let dependencies = crate::modes::dependencies_in_mode(workflow, mode_nodes);
    let top = group.map_or(&node.id, |group| &group.id);
    let mut earlier = ancestors(&dependencies, top);
    if let Some(on_failure) = &node.on_failure {
        earlier.extend(ancestors(&dependencies, &on_failure.goto));
        earlier.insert(on_failure.goto.clone());
    }
    let siblings = group
        .into_iter()
        .flat_map(|group| match &group.kind {
            NodeKind::Parallel { nodes, .. } => nodes.as_slice(),
            _ => &[],
        })
        .filter(|sibling| sibling.id != *reader);
    let writes = workflow
        .nodes
        .iter()
        .filter(|node| earlier.contains(&node.id))
        .chain(siblings)
        .any(|node| may_write(node, path, defaults));
    match writes {
        true => Reach::EarlierNodeMay,
        false => Reach::NothingEarlierCan,
    }
}

/// Every node `id` waits on, directly or through another, in the graph
/// `dependencies` describes.
fn ancestors(dependencies: &HashMap<NodeId, Vec<NodeId>>, id: &NodeId) -> HashSet<NodeId> {
    let mut seen = HashSet::new();
    let mut stack: Vec<&NodeId> = dependencies.get(id).into_iter().flatten().collect();
    while let Some(next) = stack.pop() {
        if seen.insert(next.clone()) {
            stack.extend(dependencies.get(next).into_iter().flatten());
        }
    }
    seen
}

/// Whether `node` might leave `path` written in the run's tree: it runs a
/// command or a session that can edit, or a hook, and its scope — when it
/// declares one — reaches the path. A write outside a declared scope fails
/// the node, so its reader never runs after it.
fn may_write(node: &Node, path: &str, defaults: Option<&yunta_core::Hooks>) -> bool {
    let hooks = hooks_of(node, defaults)
        .is_some_and(|hooks| !hooks.before.is_empty() || !hooks.after.is_empty());
    let writes = match &node.kind {
        NodeKind::Bash { .. } | NodeKind::Executor { .. } | NodeKind::Workflow { .. } => true,
        NodeKind::Check(builtin) => {
            !matches!(builtin, yunta_core::CheckBuiltin::FindingsGate { .. })
        }
        NodeKind::Prompt { .. } | NodeKind::Loop { .. } => node.writes_worktree(),
        // A pull request carries the run's branch; it writes nothing into
        // the tree.
        NodeKind::Gate { .. } | NodeKind::PullRequest { .. } => false,
        NodeKind::Parallel { nodes, .. } => {
            nodes.iter().any(|child| may_write(child, path, defaults))
        }
    };
    (hooks || writes) && within_scope(node, path)
}

/// The hooks a node runs: its own, or the workflow's defaults when it
/// declares none.
fn hooks_of<'a>(
    node: &'a Node,
    defaults: Option<&'a yunta_core::Hooks>,
) -> Option<&'a yunta_core::Hooks> {
    node.hooks.as_ref().or(defaults)
}

/// A scope audits the run's tree, so a path outside it — an absolute one
/// — is one any writer might reach. A node scoped to the run may write
/// whatever the run changed, which no file can know before it runs.
fn within_scope(node: &Node, path: &str) -> bool {
    let globs = node.scope.globs();
    if globs.is_empty() || Path::new(path).is_absolute() {
        return true;
    }
    yunta_core::scope_globset(globs).map_or(true, |set| set.is_match(path))
}

/// `(node, path)` for every literal, required `files:` entry of `node`
/// and of the children a `parallel` group runs, each pair once. An
/// optional entry is left out: its author already said the node goes on
/// without it.
fn literal_files(node: &Node, read: &mut Vec<(NodeId, String)>) {
    for source in &node.context {
        let ContextSpec::Files { files } = source else {
            continue;
        };
        for file in files.iter().filter(|file| !file.optional) {
            let literal = template_variables(&file.path).is_ok_and(|vars| vars.is_empty());
            let entry = (node.id.clone(), file.path.clone());
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
async fn repository_root(
    checkout: &Path,
    supervision: Supervision<'_>,
) -> Result<PathBuf, GitError> {
    let printed =
        crate::git::read(checkout, &["rev-parse", "--show-toplevel"], supervision).await?;
    Ok(PathBuf::from(printed.trim()))
}

/// How `path` is missing from the tree the run would read, or `None` when
/// it is there. Git's own failure to answer is the error, never a guess.
async fn missing(
    path: &str,
    root: &Path,
    origin: RunTreeOrigin<'_>,
    supervision: Supervision<'_>,
) -> Result<Option<MissingContextFile>, GitError> {
    if Path::new(path).is_absolute() {
        return Ok((!Path::new(path).exists()).then_some(MissingContextFile::Nowhere));
    }
    let on_disk = root.join(path).exists();
    if origin.isolation == Isolation::None {
        return Ok((!on_disk).then_some(MissingContextFile::Nowhere));
    }
    let listed = crate::git::read(
        root,
        &[
            "ls-tree",
            "-z",
            "--name-only",
            origin.base.as_str(),
            "--",
            path,
        ],
        supervision,
    )
    .await?;
    if !listed.is_empty() {
        return Ok(None);
    }
    if !on_disk {
        return Ok(Some(MissingContextFile::Nowhere));
    }
    let ignored = crate::git::ask(
        root,
        &["check-ignore", "-q", path],
        CHECK_IGNORE,
        supervision,
    )
    .await?;
    Ok(Some(if ignored {
        MissingContextFile::Ignored
    } else {
        MissingContextFile::Uncommitted
    }))
}
