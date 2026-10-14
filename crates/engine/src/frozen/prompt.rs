//! Where a workflow's prompt files are read from.
//!
//! A repo workflow's prompt file is read beside the workflow, as it always
//! was. A pack workflow's prompt file is judged by its place in the pack:
//! joined onto the workflow's own directory and folded lexically, it must
//! stay inside the pack's directory. A pack therefore cannot freeze a file
//! that the person can read but the pack does not hold.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use yunta_core::{Node, NodeId, NodeKind, PromptSource, Workflow};

use super::FrozenError;
use crate::catalog::{self, WorkflowOrigin};

/// The directory a workflow's prompt files are read from, and the pack they
/// must stay inside when the workflow came from one.
pub(crate) enum PromptRoot {
    /// A repo workflow: its prompts sit beside it, and are read as they are.
    Repo(PathBuf),
    /// A pack workflow: its prompts must stay inside `pack_dir`, the
    /// directory of the pack `pack` names. `dir` is the workflow's own
    /// directory, inside that pack.
    Pack {
        pack: String,
        pack_dir: PathBuf,
        dir: PathBuf,
    },
}

impl PromptRoot {
    /// The root for a workflow born in `workflow_dir`: its own directory,
    /// unless the catalog places that directory inside a pack.
    pub(crate) async fn of_tree(repo: &Path, workflow_dir: &Path) -> Self {
        // The repo and the workflow's directory are compared by the place they
        // resolve to: named through a link, a pack's workflow must still be
        // judged as one, and its prompts checked against the same pack root.
        let repo = resolved(repo).await;
        let dir = resolved(&absolute(&repo, workflow_dir)).await;
        match catalog::origin_of(&repo, &dir) {
            WorkflowOrigin::Repo => PromptRoot::Repo(dir),
            WorkflowOrigin::Pack {
                publisher,
                pack_name,
            } => PromptRoot::Pack {
                pack: format!("{publisher}/{pack_name}"),
                pack_dir: repo
                    .join(".yunta/packs")
                    .join(publisher.as_str())
                    .join(pack_name.as_str()),
                dir,
            },
        }
    }

    /// The path the prompt file `file`, as the workflow names it, is read
    /// from. A pack workflow's file that leaves its pack is refused.
    pub(crate) fn path_of(&self, file: &Path) -> Result<PathBuf, FrozenError> {
        match self {
            PromptRoot::Repo(dir) => Ok(dir.join(file)),
            PromptRoot::Pack {
                pack,
                pack_dir,
                dir,
            } => pack_prompt_path(pack, pack_dir, dir, file),
        }
    }
}

/// The file a node reads its prompt from, when the workflow declares one.
pub(crate) fn file_prompt(node: &Node) -> Option<&Path> {
    match &node.kind {
        NodeKind::Prompt { prompt } | NodeKind::Loop { prompt, .. } => match prompt {
            PromptSource::File(path) => Some(path.as_path()),
            PromptSource::Inline(_) => None,
        },
        _ => None,
    }
}

/// The text of every prompt file a node of `workflow` reads, keyed by node,
/// each one read from the place `root` names. The freeze reads a tree's
/// workflow and a child's the same way, through this one function.
pub(crate) async fn read_prompts(
    workflow: &Workflow,
    root: &PromptRoot,
) -> Result<BTreeMap<NodeId, String>, FrozenError> {
    let mut prompts = BTreeMap::new();
    for node in workflow.iter_nodes() {
        if let Some(file) = file_prompt(node) {
            let path = root.path_of(file)?;
            let text =
                tokio::fs::read_to_string(&path)
                    .await
                    .map_err(|source| FrozenError::Prompt {
                        node: node.id.clone(),
                        path: path.clone(),
                        source,
                    })?;
            prompts.insert(node.id.clone(), text);
        }
    }
    Ok(prompts)
}

/// `dir` as a path: a relative directory is relative to `repo`, as the
/// catalog reads it.
fn absolute(repo: &Path, dir: &Path) -> PathBuf {
    if dir.is_relative() {
        repo.join(dir)
    } else {
        dir.to_path_buf()
    }
}

/// `path` with every link it crosses resolved. What is missing is resolved
/// through its nearest existing ancestor, so a directory a link
/// reaches resolves the same way whether or not it has been written.
async fn resolved(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(real) = tokio::fs::canonicalize(existing).await {
            return missing
                .iter()
                .rev()
                .fold(real, |real, part| real.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The path `file` names from `dir`, folded against the pack's directory.
/// It is refused when `file` is absolute, when `dir` is not inside
/// `pack_dir`, or when folding its `..` components climbs above `pack_dir`.
/// The refusal names the pack and `file` as the workflow wrote it.
fn pack_prompt_path(
    pack: &str,
    pack_dir: &Path,
    dir: &Path,
    file: &Path,
) -> Result<PathBuf, FrozenError> {
    let refused = || FrozenError::Escapes {
        pack: pack.to_string(),
        path: file.display().to_string(),
    };
    if file.is_absolute() {
        return Err(refused());
    }
    let inside = dir
        .strip_prefix(pack_dir)
        .map_err(|_| refused())?
        .join(file);
    let folded = fold(&inside).ok_or_else(refused)?;
    Ok(pack_dir.join(folded))
}

/// `path` with its `.` and `..` components folded away, lexically. `None`
/// when a `..` would climb above the path's first component, or when the
/// path is absolute.
fn fold(path: &Path) -> Option<PathBuf> {
    let mut kept: Vec<&OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => kept.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                kept.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(kept.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK_DIR: &str = "/repo/.yunta/packs/pub/tools";

    #[test]
    fn a_prompt_that_climbs_within_its_pack_is_read_from_the_pack() {
        let pack = Path::new(PACK_DIR);
        let dir = pack.join("workflows");
        let read = pack_prompt_path("pub/tools", pack, &dir, Path::new("../prompts/plan.md"));
        assert_eq!(read.unwrap(), pack.join("prompts/plan.md"));
    }

    #[test]
    fn a_prompt_in_the_pack_root_stays_where_it_names() {
        let pack = Path::new(PACK_DIR);
        let read = pack_prompt_path("pub/tools", pack, pack, Path::new("./plan.md"));
        assert_eq!(read.unwrap(), pack.join("plan.md"));
    }

    #[test]
    fn a_prompt_that_climbs_above_its_pack_is_refused_naming_the_pack_and_the_path() {
        let pack = Path::new(PACK_DIR);
        let dir = pack.join("workflows");
        let refused = pack_prompt_path(
            "pub/tools",
            pack,
            &dir,
            Path::new("../../../../../secret.txt"),
        );
        match refused {
            Err(FrozenError::Escapes {
                pack: named,
                path: written,
            }) => {
                assert_eq!(named, "pub/tools");
                assert_eq!(written, "../../../../../secret.txt");
            }
            other => panic!("expected the prompt refused, got {other:?}"),
        }
    }

    #[test]
    fn an_absolute_prompt_is_refused_whatever_directory_it_is_named_from() {
        let pack = Path::new(PACK_DIR);
        let refused = pack_prompt_path("pub/tools", pack, pack, Path::new("/etc/passwd"));
        assert!(
            matches!(refused, Err(FrozenError::Escapes { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn a_repo_workflow_prompt_is_read_beside_it_and_not_judged() {
        let root = PromptRoot::Repo(PathBuf::from("/repo/.yunta/workflows"));
        let read = root.path_of(Path::new("../../outside.md"));
        assert_eq!(
            read.unwrap(),
            PathBuf::from("/repo/.yunta/workflows/../../outside.md")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_pack_workflow_named_through_a_link_is_judged_as_a_pack_workflow() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        let workflows = repo.join(".yunta/packs/pub/tools/workflows");
        tokio::fs::create_dir_all(&workflows).await.unwrap();
        let link = root.path().join("linked");
        std::os::unix::fs::symlink(&workflows, &link).unwrap();
        let root_of = PromptRoot::of_tree(&repo, &link).await;
        let refused = root_of.path_of(Path::new("../../../../../secret.txt"));
        assert!(
            matches!(refused, Err(FrozenError::Escapes { .. })),
            "{refused:?}"
        );
    }

    #[test]
    fn fold_applies_dots_and_refuses_a_climb_above_its_first_component() {
        assert_eq!(
            fold(Path::new("a/b/../c/./d")),
            Some(PathBuf::from("a/c/d"))
        );
        assert_eq!(fold(Path::new("a/../..")), None);
    }
}
