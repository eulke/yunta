//! Manifest resolution — the IO shell that freezes
//! a [`Manifest`]. Reading prompt files and asking git for the base
//! commit happen here, once, at run creation; everything downstream
//! operates on the frozen value and never goes back to disk.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use thiserror::Error;
use yunta_core::{
    content_hash, ConfigLayer, Manifest, Node, NodeId, NodeKind, PromptSource, Workflow,
};

use crate::inputs::{resolve_inputs, InputsError};

/// Version of the manifest's own schema.
const MANIFEST_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("node `{node}` references prompt file `{path}` which cannot be read")]
    PromptFile {
        node: NodeId,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to resolve the base commit: git {args} in `{cwd}`: {detail}")]
    Git {
        args: String,
        cwd: PathBuf,
        detail: String,
    },
    #[error(transparent)]
    Inputs(#[from] InputsError),
}

/// Freezes a manifest from already-parsed sources.
///
/// `workflow_dir` is the directory of the workflow file — `prompt:
/// {file: ...}` paths resolve relative to it. `repo` is the
/// working tree whose `HEAD` becomes the base commit, and also the base
/// a `path`-typed input in `provided_inputs` resolves against:
/// inputs are validated before any worktree exists, so there is nowhere
/// else for a relative path to mean.
pub fn build_manifest(
    workflow: &Workflow,
    config: &ConfigLayer,
    workflow_dir: &Path,
    repo: &Path,
    provided_inputs: &HashMap<String, String>,
) -> Result<Manifest, ManifestError> {
    let mut workflow = workflow.clone();
    expand_runner_fanout(&mut workflow);
    expand_implicit_dependencies(&mut workflow);

    let inputs = resolve_inputs(&workflow.inputs, provided_inputs, repo)?;

    let mut prompts = BTreeMap::new();
    for node in workflow.iter_nodes() {
        freeze_prompt(node, workflow_dir, &mut prompts)?;
    }

    let base_commit = git_line(repo, &["rev-parse", "HEAD"])?;
    let base_branch = git_line(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;

    Ok(Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        yunta_version: env!("CARGO_PKG_VERSION").to_string(),
        workflow_hash: content_hash(&workflow),
        config_hash: content_hash(config),
        workflow,
        config: config.clone(),
        inputs,
        prompts,
        base_branch,
        base_commit,
        isolation: config.resolved_isolation(),
        max_parallel_nodes: config.resolved_max_parallel_nodes(),
        // The resolved state roots live in the shell that knows them
        // (the CLI's project resolution) — it fills this in before
        // `create_run` freezes the manifest. `None` here keeps
        // library callers (tests) on the fallback-to-current-config
        // path, which is also the tolerant reading of old manifests.
        paths: None,
        pack: pack_provenance(repo, workflow_dir),
    })
}

/// Which pack (and exactly which version) the
/// top-level workflow being frozen came from, if any — best-effort,
/// since this field is provenance for `yunta status`/`receipt`, never
/// load-bearing for the run's own correctness (that guarantee comes
/// from `workflow` itself being embedded whole, and from `workflow_hash`
/// covering it). `None` for a repo-origin workflow, and also whenever
/// the pack's own files can't be read back at this exact moment — a
/// self-inconsistent local state (`.yunta/packs/<publisher>/<name>/`
/// existing without a readable `pack.yaml`) that this function has no
/// better answer for than omitting provenance rather than failing the
/// run.
fn pack_provenance(repo: &Path, workflow_dir: &Path) -> Option<yunta_core::PackProvenance> {
    let crate::catalog::WorkflowOrigin::Pack {
        publisher,
        pack_name,
    } = crate::catalog::origin_of(repo, workflow_dir)
    else {
        return None;
    };
    let pack_dir = repo.join(".yunta/packs").join(&publisher).join(&pack_name);
    let manifest_text = std::fs::read_to_string(pack_dir.join("pack.yaml")).ok()?;
    let manifest: yunta_core::PackManifest = yunta_core::yaml::parse(&manifest_text).ok()?;

    let commit = std::fs::read_to_string(repo.join(".yunta/yunta.lock"))
        .ok()
        .and_then(|text| yunta_core::yaml::parse::<yunta_core::PackLock>(&text).ok())
        .and_then(|lock| {
            lock.packs
                .get(&yunta_core::PackLock::key(&publisher, &pack_name))
                .map(|entry| entry.commit.clone())
        });

    Some(yunta_core::PackProvenance {
        publisher,
        name: pack_name,
        version: manifest.version,
        commit,
    })
}

/// `context: [{ artifact: { node, name } }]` creates an *implicit*
/// `depends_on` edge onto `node` — folded into the ordinary field here,
/// once, so `check`'s cycle detection and the scheduler's own readiness
/// calculation (both already only ever read `Node.depends_on`) need zero
/// awareness of `context:` existing at all. `check()` calls this too
/// (its own copy of the workflow, never the manifest's), so a cycle
/// created purely by two nodes' context-artifact references is still
/// caught statically rather than deadlocking a real run. Idempotent: a
/// node that already lists the referenced node explicitly gets no
/// duplicate.
/// A node with `runners: [a, b]` becomes one `<id>@<role>`
/// node per role — **statically, in the manifest**, before anything
/// runs: the fan-out is visible in `status`, each expanded node
/// resolves its own runner and renders its own `{{runner.role}}`, and
/// the scheduler needs zero fan-out awareness. Every reference to the
/// original id follows the expansion: downstream `depends_on` rewires
/// onto all siblings, and mode include lists name them all (so a mode
/// that covered `review` still covers the whole review). Re-route and
/// gate targets onto a fan-out node are check errors — there is no
/// unambiguous "return control to review" once review is many nodes —
/// so this function never sees one.
pub(crate) fn expand_runner_fanout(workflow: &mut Workflow) {
    let mut expansion: std::collections::HashMap<yunta_core::NodeId, Vec<yunta_core::NodeId>> =
        std::collections::HashMap::new();
    let mut nodes = Vec::with_capacity(workflow.nodes.len());
    for node in workflow.nodes.drain(..) {
        if node.runners.is_empty() {
            nodes.push(node);
            continue;
        }
        let mut expanded_ids = Vec::new();
        for role in &node.runners {
            let mut sibling = node.clone();
            sibling.id = format!("{}@{role}", node.id).into();
            sibling.runner = Some(role.clone());
            sibling.runners = Vec::new();
            expanded_ids.push(sibling.id.clone());
            nodes.push(sibling);
        }
        expansion.insert(node.id.clone(), expanded_ids);
    }
    for node in &mut nodes {
        let mut rewired = Vec::with_capacity(node.depends_on.len());
        for dep in node.depends_on.drain(..) {
            match expansion.get(&dep) {
                Some(siblings) => rewired.extend(siblings.iter().cloned()),
                None => rewired.push(dep),
            }
        }
        node.depends_on = rewired;
    }
    if let Some(modes) = &mut workflow.modes {
        for spec in modes.values_mut() {
            if let yunta_core::ModeInclude::Nodes(included) = &mut spec.include {
                let mut rewritten = Vec::with_capacity(included.len());
                for id in included.drain(..) {
                    match expansion.get(&id) {
                        Some(siblings) => rewritten.extend(siblings.iter().cloned()),
                        None => rewritten.push(id),
                    }
                }
                *included = rewritten;
            }
        }
    }
    workflow.nodes = nodes;
}

pub(crate) fn expand_implicit_dependencies(workflow: &mut Workflow) {
    for node in &mut workflow.nodes {
        expand_implicit_dependencies_in(node);
    }
}

fn expand_implicit_dependencies_in(node: &mut Node) {
    if let NodeKind::Parallel { nodes, .. } = &mut node.kind {
        for child in nodes {
            expand_implicit_dependencies_in(child);
        }
    }
    // A mount is a read of the referenced node's outcome, so
    // it orders behind it exactly like a context artifact does — and
    // it's this edge that guarantees the source node has already
    // finished at the time the child is born and the copy happens.
    let mut implied: Vec<NodeId> = Vec::new();
    if let NodeKind::Workflow { mounts, .. } = &node.kind {
        implied.extend(mounts.iter().map(|mount| mount.artifact.node.clone()));
    }
    for spec in &node.context {
        if let yunta_core::ContextSpec::Artifact { artifact } = spec {
            // A node-less reference reads this run's own
            // artifacts dir — no producer to order behind.
            if let Some(referenced) = &artifact.node {
                implied.push(referenced.clone());
            }
        }
    }
    for referenced in implied {
        if !node.depends_on.contains(&referenced) {
            node.depends_on.push(referenced);
        }
    }
}

/// Freezes one node's own file prompt (if any) — `build_manifest` walks
/// every node (`parallel` children included, via `iter_nodes`): a
/// child's `prompt: {file: ...}` needs the same freeze-at-creation
/// guarantee as a top-level node's, since it's dispatched
/// through the identical `execute_node`.
fn freeze_prompt(
    node: &Node,
    workflow_dir: &Path,
    prompts: &mut BTreeMap<NodeId, String>,
) -> Result<(), ManifestError> {
    if let NodeKind::Prompt { prompt } | NodeKind::Loop { prompt, .. } = &node.kind {
        if let PromptSource::File(path) = prompt {
            let full_path = workflow_dir.join(path);
            let content = std::fs::read_to_string(&full_path).map_err(|source| {
                ManifestError::PromptFile {
                    node: node.id.clone(),
                    path: full_path.clone(),
                    source,
                }
            })?;
            prompts.insert(node.id.clone(), content);
        }
    }
    Ok(())
}

fn git_line(repo: &Path, args: &[&str]) -> Result<String, ManifestError> {
    let git_error = |detail: String| ManifestError::Git {
        args: args.join(" "),
        cwd: repo.to_path_buf(),
        detail,
    };

    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| git_error(e.to_string()))?;

    if !output.status.success() {
        return Err(git_error(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
