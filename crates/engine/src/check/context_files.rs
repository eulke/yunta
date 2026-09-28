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

/// What the `files:` walk found: the paths no node could supply before
/// their reader, and the ones only a node that runs earlier could.
#[derive(Debug, Default)]
pub struct ContextFilesCheck {
    pub errors: Vec<CheckError>,
    pub warnings: Vec<CheckWarning>,
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
/// renders it, and is left to the run; so is anything git cannot answer
/// here.
pub async fn check_context_files(
    workflow: &Workflow,
    mode_nodes: Option<&HashSet<NodeId>>,
    origin: RunTreeOrigin<'_>,
    supervision: Supervision<'_>,
) -> ContextFilesCheck {
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
        return found;
    }
    let root = repository_root(origin.checkout, supervision).await;
    for (node, path) in read {
        if let Some(missing) = missing(&path, root.as_deref(), origin, supervision).await {
            let base = (origin.isolation == Isolation::Worktree && !Path::new(&path).is_absolute())
                .then(|| origin.base.abbreviated().to_string());
            match reach(workflow, &node, &path, mode_nodes) {
                Reach::EarlierNodeMay => found.warnings.push(CheckWarning::ContextFileMissing {
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
    found
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
        NodeKind::Prompt { .. } | NodeKind::Loop { .. } => {
            node.permissions != Some(yunta_core::NodePermissions::ReadOnly)
        }
        NodeKind::Gate { .. } => false,
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
/// — is one any writer might reach.
fn within_scope(node: &Node, path: &str) -> bool {
    if node.scope.is_empty() || Path::new(path).is_absolute() {
        return true;
    }
    yunta_core::scope_globset(&node.scope).map_or(true, |set| set.is_match(path))
}

/// `(node, path)` for every literal, required `files:` entry of `node`
/// and of the children a `parallel` group runs, each pair once. An
/// optional entry is left out: its author already said the node goes on
/// without it (D186).
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

/// The sentence for a missing `files:` path: what is missing, why the run
/// would not see it, whether anything could still write it, and what to
/// do — each shape its own remedy.
pub(super) fn missing_sentence(
    node: &NodeId,
    path: &str,
    base: Option<&str>,
    missing: &MissingContextFile,
    reach: Reach,
) -> String {
    use MissingContextFile as M;
    let reads = format!("node `{node}` reads `{path}` (a `files:` context source)");
    let stops = match reach {
        Reach::EarlierNodeMay => {
            format!("unless a node before it writes the file, `{node}` stops there")
        }
        Reach::NothingEarlierCan => format!(
            "`{node}` stops there every time, since no node that runs before it can write the file"
        ),
    };
    let optional = "or declare the entry `optional: true` if the node can do without it";
    match (missing, base) {
        (M::Nowhere, Some(base)) => format!(
            "{reads}, which commit `{base}` — the one a run starts from — does not hold: \
             {stops}; commit the file first, {optional}"
        ),
        (M::Nowhere, None) => format!("{reads}, which does not exist: {stops}; {optional}"),
        (M::Uncommitted, _) => format!(
            "{reads}, which is in your checkout but not committed: a run starts from commit \
             `{}` and never sees it, so {stops}; commit it first",
            base.unwrap_or("HEAD")
        ),
        (M::Ignored, _) => format!(
            "{reads}, which git ignores: a run's tree never carries an ignored file, so \
             {stops}; add it with `git add -f`, or read a file git tracks"
        ),
    }
}
