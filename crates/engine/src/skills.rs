//! Skill-name resolution: `skills: [names]` on a node (or
//! `node_defaults`) plus `skills.always` from config, resolved against
//! `skills.paths` — repo first, the same layer order knowledge uses.
//! Resolution is the engine's; *mounting* is the adapter's native
//! mechanism (`SessionRequest.skills`), and an adapter without one
//! degrades with `capability_degraded`, never a fatal error — a
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
            .find(|candidate| candidate.is_dir())
            // Repo/configured paths never find a
            // namespaced name as a literal subdirectory in practice, so
            // this only ever fires for `publisher/skill` — a publisher's
            // vendored packs are the fallback layer, never the first one
            // (a repo directory that happens to occupy that path still
            // wins, same shadowing rule `use:` follows).
            .or_else(|| resolve_pack_skill(worktree, name));
        match found {
            Some(dir) => resolved.push(dir),
            None => {
                return Err(format!(
                    "skill `{name}` not found under {} — add the directory, fix \
                     `skills.paths` in the config, or install the pack that declares it",
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

/// `publisher/skill` — a publisher's installed packs share one
/// flat skill namespace, same rule [`crate::catalog::resolve_workflow`]
/// applies to workflows: a directory basename match against some pack's
/// declared `contents.skills`, ambiguity between two packs left
/// unresolved (reported as "not found" here — a workflow-level
/// `Ambiguous` error doesn't apply to a mount-time lookup with no
/// `check`-time diagnostic surface of its own).
fn resolve_pack_skill(worktree: &Path, name: &str) -> Option<PathBuf> {
    let (publisher, skill) = name.split_once('/')?;
    let mut found: Option<PathBuf> = None;
    for (pack_dir, manifest) in crate::catalog::packs_for_publisher(worktree, publisher) {
        for declared in &manifest.contents.skills {
            let trimmed = declared.trim_end_matches('/');
            let stem = Path::new(trimmed).file_name().and_then(|s| s.to_str());
            if stem == Some(skill) {
                if found.is_some() {
                    return None; // ambiguous across packs — no silent first-match
                }
                found = Some(pack_dir.join(trimmed));
            }
        }
    }
    found
}

/// A configured skills path is absolute, or relative to the worktree;
/// a `~` was expanded when the config layer was loaded.
fn expand(path: &Path, worktree: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        worktree.join(path)
    }
}
