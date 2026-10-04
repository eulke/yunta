//! What a task may write beyond the scope it declared because a shape it
//! owns is named there.
//!
//! A task that changes a shape changes what names it: the caller of a
//! signature it widens, the match on an enum it grows. The plan declares
//! the shape and its owner; which files name it is a fact of the tree the
//! run works in, so the engine reads it there — rather than asking the
//! planner to foresee every caller, or a person to allow each one once
//! the build points at it.

use std::collections::BTreeSet;
use std::path::Path;

use yunta_core::events::{EventPayload, ScopeDerivedPayload, ScopeEvent};
use yunta_core::{CommitSha, NodeId, ScopeGlob, Shape, Task, TasksFile};

use crate::process::Supervision;
use crate::run::RunError;
use crate::run_log::RunLog;

/// States, for each task of `document`, every file of `tree` that names a
/// shape the task owns and its declared scope leaves out. Once per change:
/// a task whose reach the log already states as this one is left alone,
/// and one a recut gave other shapes is stated again, replacing it.
pub(crate) async fn derive_reach(
    log: &RunLog<'_>,
    node: Option<&NodeId>,
    document: &TasksFile,
    tree: &Path,
    supervision: Supervision<'_>,
) -> Result<(), RunError> {
    let stated = crate::replay::derive(&log.events().await?).grants;
    let followed = document.shapes.iter().any(|shape| identifier(&shape.name));
    let restated = document
        .tasks
        .iter()
        .any(|task| stated.derived_for(&task.id).is_some());
    if !followed && !restated {
        return Ok(());
    }
    let at = crate::worktree::head_commit(tree, supervision).await?;
    for task in &document.tasks {
        let reach = reach_of(task, &document.shapes, tree, &at, supervision).await?;
        let unchanged = match stated.derived_for(&task.id) {
            Some(before) => {
                before.paths == reach.paths
                    && before.shapes == reach.shapes
                    && before.common == reach.common
            }
            None => reach.paths.is_empty() && reach.common.is_empty(),
        };
        if unchanged {
            continue;
        }
        log.record(node, EventPayload::Scope(ScopeEvent::Derived(reach)))
            .await?;
    }
    Ok(())
}

/// The files at `at` that name a shape `task` owns, outside its declared
/// scope, and the shapes named too widely to follow.
async fn reach_of(
    task: &Task,
    shapes: &[Shape],
    tree: &Path,
    at: &CommitSha,
    supervision: Supervision<'_>,
) -> Result<ScopeDerivedPayload, RunError> {
    let declared = yunta_core::scope_globset(&task.scope).ok();
    let mut files = BTreeSet::new();
    let mut named = Vec::new();
    let mut common = Vec::new();
    let owned = shapes
        .iter()
        .filter(|shape| shape.owner == task.id && identifier(&shape.name));
    for shape in owned {
        let Some(kind) = Path::new(&shape.file).extension() else {
            continue;
        };
        let pathspec = format!("*.{}", kind.to_string_lossy());
        let naming =
            crate::git::files_naming(tree, at.as_str(), &shape.name, &pathspec, supervision)
                .await?;
        // A name this many files carry is a word of the language more
        // than a shape of the plan: following it would let the owner
        // write half the tree.
        if naming.len() > 20 {
            common.push(shape.name.clone());
            continue;
        }
        let beyond: Vec<String> = naming
            .into_iter()
            .filter(|file| !declared.as_ref().is_some_and(|set| set.is_match(file)))
            .collect();
        if !beyond.is_empty() {
            named.push(shape.name.clone());
            files.extend(beyond);
        }
    }
    Ok(ScopeDerivedPayload {
        task_id: task.id.clone(),
        paths: files
            .iter()
            .filter_map(|file| ScopeGlob::exact(Path::new(file)).ok())
            .collect(),
        shapes: named,
        common,
        at: at.clone(),
    })
}

/// Whether `name` is one word of code, as `git grep -w` reads a word: what
/// a shape the code calls by name is, and a file format the plan describes
/// is not.
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::identifier;

    #[test]
    fn only_a_word_of_code_is_followed() {
        assert!(identifier("build_manifest"));
        assert!(identifier("PackScope"));
        assert!(!identifier("Bench::run"));
        assert!(!identifier("pack.lock format"));
        assert!(!identifier("2fa"));
    }
}
