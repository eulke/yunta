//! Skill-name resolution: `skills: [names]` on a node (or
//! `node_defaults`) plus `skills.always` from config, resolved against
//! `skills.paths` — repo first, the same layer order knowledge uses.
//! Resolution is the engine's; *mounting* is the adapter's native
//! mechanism (`SessionRequest.skills`), and an adapter without one
//! degrades with `capability_degraded`, never a fatal error — a
//! skill is added instruction, not correctness.
//!
//! The walk that freezes a run resolves each name once, at birth. A node of
//! a run that predates frozen sets resolves its names from the checkout,
//! through the same lookup.

use std::path::{Path, PathBuf};

use yunta_core::{ConfigLayer, Env, Node, Publisher, SkillName, Workflow};

use crate::catalog::{pack_declaring, DeclaredPath};

/// The names one node mounts: `skills.always` first, then the node's
/// own list (or `node_defaults.skills` when the node declares none —
/// the same replace-wholesale inheritance as hooks), deduplicated.
pub(crate) fn skill_names(
    config: &ConfigLayer,
    workflow: &Workflow,
    node: &Node,
) -> Vec<SkillName> {
    let own: &[SkillName] = if node.skills.is_empty() {
        workflow
            .node_defaults
            .as_ref()
            .map(|defaults| defaults.skills.as_slice())
            .unwrap_or(&[])
    } else {
        &node.skills
    };
    let always: &[SkillName] = config
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

/// A skill name that resolves to no directory.
#[derive(Debug, thiserror::Error)]
pub enum SkillsError {
    #[error("{}", yunta_core::events::skill_not_found(.name, .searched))]
    NotFound {
        name: SkillName,
        searched: Vec<PathBuf>,
    },
}

/// Where a skill name is: a directory under a configured skills path, or
/// the directory a pack declares.
pub(crate) enum Location {
    Dir(PathBuf),
    Pack(DeclaredPath),
}

/// Resolves one skill name to its directory in the checkout at `worktree`,
/// or says which name failed and where it looked. Relative search paths
/// resolve against `worktree` (the run's checkout carries the repo's own
/// `.yunta/skills`); `~/` expands against `$HOME`. `ambient` is the
/// environment the user's packs are found in.
pub(crate) fn locate(
    config: &ConfigLayer,
    worktree: &Path,
    ambient: Option<&Env>,
    name: &SkillName,
) -> Result<Location, SkillsError> {
    let roots = search_roots(config, worktree);
    let found = roots
        .iter()
        .map(|root| root.join(name.as_str()))
        .find(|candidate| candidate.is_dir());
    if let Some(dir) = found {
        return Ok(Location::Dir(dir));
    }
    // Repo/configured paths never find a namespaced name as a literal
    // subdirectory in practice, so this only ever fires for
    // `publisher/skill` — a publisher's vendored packs are the fallback
    // layer, never the first one (a repo directory that happens to occupy
    // that path still wins, same shadowing rule `use:` follows).
    resolve_pack_skill(worktree, ambient, name.as_str())
        .map(Location::Pack)
        .ok_or_else(|| SkillsError::NotFound {
            name: name.clone(),
            searched: roots,
        })
}

/// The directories a skill name is searched under, in order: the configured
/// `skills.paths`, else `.yunta/skills`.
fn search_roots(config: &ConfigLayer, worktree: &Path) -> Vec<PathBuf> {
    let declared_paths: Vec<PathBuf> = config
        .skills
        .as_ref()
        .filter(|skills| !skills.paths.is_empty())
        .map(|skills| skills.paths.clone())
        .unwrap_or_else(|| vec![PathBuf::from(".yunta/skills")]);
    declared_paths
        .iter()
        .map(|path| expand(path, worktree))
        .collect()
}

/// `publisher/skill` — a publisher's installed packs share one
/// flat skill namespace, same rule [`crate::catalog::resolve_workflow`]
/// applies to workflows: a directory basename match against some pack's
/// declared `contents.skills`, from the first store whose packs declare
/// one, ambiguity between two packs of that store left unresolved (reported
/// as "not found" here — a workflow-level `Ambiguous` error doesn't apply to
/// a mount-time lookup with no `check`-time diagnostic surface of its own).
fn resolve_pack_skill(worktree: &Path, ambient: Option<&Env>, name: &str) -> Option<DeclaredPath> {
    let (publisher, skill) = name.split_once('/')?;
    let publisher = publisher.parse::<Publisher>().ok()?;
    pack_declaring(worktree, ambient, &publisher, |manifest| {
        let named =
            |trimmed: &&str| Path::new(trimmed).file_name().and_then(|s| s.to_str()) == Some(skill);
        manifest
            .contents
            .skills
            .iter()
            .map(|declared| declared.trim_end_matches('/'))
            .filter(named)
            .map(str::to_string)
            .collect()
    })
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
