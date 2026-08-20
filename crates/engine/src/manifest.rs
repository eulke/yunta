//! Manifest resolution (T1.4, Contrato §2.1) — the IO shell that freezes
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

/// Version of the manifest's own schema (D07).
const MANIFEST_SCHEMA_VERSION: u32 = 1;

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

/// Freezes a manifest from already-parsed sources (Contrato §2.1).
///
/// `workflow_dir` is the directory of the workflow file — `prompt:
/// {file: ...}` paths resolve relative to it (§9.3). `repo` is the
/// working tree whose `HEAD` becomes the base commit, and also the base
/// a `path`-typed input in `provided_inputs` resolves against (T1.5):
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
    expand_implicit_dependencies(&mut workflow);

    let inputs = resolve_inputs(&workflow.inputs, provided_inputs, repo)?;

    let mut prompts = BTreeMap::new();
    for node in &workflow.nodes {
        freeze_prompts(node, workflow_dir, &mut prompts)?;
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
    })
}

/// §9's `context: [{ artifact: { node, name } }]` creates an *implicit*
/// `depends_on` edge onto `node` — folded into the ordinary field here,
/// once, so `check`'s cycle detection and the scheduler's own readiness
/// calculation (both already only ever read `Node.depends_on`) need zero
/// awareness of `context:` existing at all. `check()` calls this too
/// (its own copy of the workflow, never the manifest's), so a cycle
/// created purely by two nodes' context-artifact references is still
/// caught statically rather than deadlocking a real run. Idempotent: a
/// node that already lists the referenced node explicitly gets no
/// duplicate.
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
    for spec in &node.context {
        if let yunta_core::ContextSpec::Artifact { artifact } = spec {
            if !node.depends_on.contains(&artifact.node) {
                node.depends_on.push(artifact.node.clone());
            }
        }
    }
}

/// Freezes `node`'s own file prompt (if any) and recurses into a
/// `parallel` node's children (T4.6) — a child's `prompt: {file: ...}`
/// needs the same freeze-at-creation guarantee (§2.1) as a top-level
/// node's, since it's dispatched through the identical `execute_node`.
fn freeze_prompts(
    node: &Node,
    workflow_dir: &Path,
    prompts: &mut BTreeMap<NodeId, String>,
) -> Result<(), ManifestError> {
    match &node.kind {
        NodeKind::Prompt { prompt } | NodeKind::Loop { prompt, .. } => {
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
        NodeKind::Bash { .. } | NodeKind::Check { .. } | NodeKind::Executor { .. } => {}
        NodeKind::Parallel { nodes, .. } => {
            for child in nodes {
                freeze_prompts(child, workflow_dir, prompts)?;
            }
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
