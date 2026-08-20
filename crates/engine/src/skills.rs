//! Skill-name resolution (DI-13, D47): `skills: [names]` on a node (or
//! `node_defaults`) plus `skills.always` from config, resolved against
//! `skills.paths` — repo first, the same layer order knowledge uses.
//! Resolution is the engine's; *mounting* is the adapter's native
//! mechanism (`SessionRequest.skills`), and an adapter without one
//! degrades with `capability_degraded`, never a fatal error (A6) — a
//! skill is added instruction, not correctness.

use std::path::{Path, PathBuf};

use yunta_core::{ConfigLayer, Node, Workflow};

/// The names one node mounts: `skills.always` first, then the node's
/// own list (or `node_defaults.skills` when the node declares none —
/// the same replace-wholesale inheritance as hooks), deduplicated.
fn skill_names(config: &ConfigLayer, workflow: &Workflow, node: &Node) -> Vec<String> {
    let own: &[String] = if node.skills.is_empty() {
        workflow
            .node_defaults
            .as_ref()
            .map(|defaults| defaults.skills.as_slice())
            .unwrap_or(&[])
    } else {
        &node.skills
    };
    let always: &[String] = config
        .skills
        .as_ref()
        .map(|skills| skills.always.as_slice())
        .unwrap_or(&[]);
    let mut names = Vec::new();
    for name in always.iter().chain(own) {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    names
}

/// Resolves every skill name to an existing directory, or says exactly
/// which name failed and where it looked. Relative search paths resolve
/// against `worktree` (the run's checkout carries the repo's own
/// `.yunta/skills`); `~/` expands against `$HOME`.
pub fn resolve_skills(
    config: &ConfigLayer,
    workflow: &Workflow,
    node: &Node,
    worktree: &Path,
) -> Result<Vec<PathBuf>, String> {
    let names = skill_names(config, workflow, node);
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let declared_paths: Vec<PathBuf> = config
        .skills
        .as_ref()
        .filter(|skills| !skills.paths.is_empty())
        .map(|skills| skills.paths.clone())
        .unwrap_or_else(|| vec![PathBuf::from(".yunta/skills")]);
    let search_roots: Vec<PathBuf> = declared_paths
        .iter()
        .map(|path| expand(path, worktree))
        .collect();

    let mut resolved = Vec::new();
    for name in &names {
        let found = search_roots
            .iter()
            .map(|root| root.join(name))
            .find(|candidate| candidate.is_dir());
        match found {
            Some(dir) => resolved.push(dir),
            None => {
                return Err(format!(
                    "skill `{name}` not found under {} — add the directory or fix \
                     `skills.paths` in the config",
                    search_roots
                        .iter()
                        .map(|root| format!("`{}`", root.display()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    Ok(resolved)
}

fn expand(path: &Path, worktree: &Path) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        worktree.join(path)
    }
}
