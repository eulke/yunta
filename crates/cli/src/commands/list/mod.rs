//! `yunta list`: the repo's own catalog of workflows — name,
//! description, declared inputs — with `--runs` switching to local runs
//! instead. Both answer the same question without a server: "what's
//! here, and where does it stand."
//!
//! The catalog is two layers: the repo's own `.yunta/workflows/`, then
//! every installed pack's declared `contents.workflows`, addressed
//! `publisher/name` — a bare repo name never collides with a pack entry
//! since the two are printed and looked up under different keys.
//!
//! The runs view lives in [`runs`], which renders them as an inbox.

pub(crate) mod inbox;
mod runs;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use yunta_core::Workflow;
use yunta_render::page::{
    Catalog, OfferedWorkflow, ReadableWorkflow, Signpost, UnreadableWorkflow,
};

use crate::context::Context;
use crate::error::CliError;
use crate::output::show;
use crate::output::Outcome;
use surco::doc::Depth;
use surco::look::Look;

pub use runs::list_runs;

/// `yunta` alone: in a repository, its field — the runs of that
/// repository; anywhere else, where Yunta works and what to type.
pub async fn field(look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    if inbox::in_a_repository(&ctx).await? {
        return list_runs(false, Depth::Glance, crate::output::Mode::Human, look).await;
    }
    show(look, &Signpost);
    Ok(Outcome::Success)
}

/// One catalog entry ready to render — `display_name` already carries
/// the `publisher/` prefix for a pack entry, nothing else needs to know
/// where it came from.
struct CatalogEntry {
    display_name: String,
    path: PathBuf,
}

pub async fn list_workflows(look: &Look) -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;

    // Best-effort — a project with no state root yet (never ran
    // anything) simply shows no estimation, same as "fewer than three
    // runs" does; neither is an error worth refusing the catalog over.
    let catalog = read_catalog(&cwd, Context::load().ok().as_ref()).await;
    show(look, &catalog);
    Ok(Outcome::Success)
}

/// What the catalog offers under `cwd`, with each workflow's history read
/// from `history_source` when there is one.
pub(crate) async fn read_catalog(cwd: &Path, history_source: Option<&Context>) -> Catalog {
    let mut entries = repo_catalog_entries(cwd);
    let shadowed: HashSet<String> = entries.iter().map(|e| e.display_name.clone()).collect();
    let (pack_entries, broken_packs) = pack_catalog_entries(cwd);
    entries.extend(
        pack_entries
            .into_iter()
            .filter(|e| !shadowed.contains(&e.display_name)),
    );
    let mut workflows = Vec::new();
    for entry in entries {
        workflows.push(offered(entry, history_source).await);
    }
    Catalog {
        broken_packs: broken_packs.iter().map(ToString::to_string).collect(),
        workflows_dir: cwd.join(".yunta/workflows"),
        packs_dir: cwd.join(".yunta/packs"),
        offered: workflows,
    }
}

/// `entry` as the catalog offers it: its workflow and how its past runs
/// usually went, or why its file does not read.
async fn offered(entry: CatalogEntry, history_source: Option<&Context>) -> OfferedWorkflow {
    let workflow = match read_entry(&entry).await {
        Ok(workflow) => {
            let usually = match history_source {
                Some(ctx) => estimation(ctx, &workflow).await,
                None => None,
            };
            Ok(ReadableWorkflow { workflow, usually })
        }
        Err(why) => Err(why),
    };
    OfferedWorkflow {
        name: entry.display_name,
        workflow,
    }
}

/// The workflow `entry` names, or the reason its file does not read.
async fn read_entry(entry: &CatalogEntry) -> Result<Workflow, UnreadableWorkflow> {
    let contents = tokio::fs::read_to_string(&entry.path)
        .await
        .map_err(|e| UnreadableWorkflow::File(e.to_string()))?;
    yunta_core::workflow::read::read(&contents, &entry.path).map_err(|report| {
        UnreadableWorkflow::NotAWorkflow(yunta_render::blocks::diagnostic::document_problems(
            &report,
            Some(&contents),
        ))
    })
}

/// How the workflow's past runs usually went, when its history says.
async fn estimation(
    ctx: &Context,
    workflow: &Workflow,
) -> Option<yunta_replay::history::PriorEstimation> {
    let history = super::stats::summaries(&super::stats::history(ctx, &workflow.name).await);
    yunta_replay::history::PriorEstimation::of(&history)
}

/// Every workflow the catalog offers that reads back — the repo's own,
/// then every installed pack's — by the name a person runs it with.
/// What does not read is left to `list` and `check` to name.
pub(crate) fn catalog_workflows(cwd: &Path) -> Vec<(String, yunta_core::Workflow)> {
    repo_catalog_entries(cwd)
        .into_iter()
        .chain(pack_catalog_entries(cwd).0)
        .filter_map(|entry| {
            crate::load_workflow(&entry.path)
                .ok()
                .map(|workflow| (entry.display_name, workflow))
        })
        .collect()
}

fn repo_catalog_entries(cwd: &std::path::Path) -> Vec<CatalogEntry> {
    let workflows_dir = cwd.join(".yunta/workflows");
    let mut paths = Vec::new();
    walk_yaml_files(&workflows_dir, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let display_name = path
                .strip_prefix(&workflows_dir)
                .unwrap_or(&path)
                .with_extension("")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            CatalogEntry { display_name, path }
        })
        .collect()
}

/// Recursively collects every `.yaml`/`.yml` file under `dir` — a repo
/// workflow that shadows a namespaced pack entry (e.g.
/// `.yunta/workflows/acme/review.yaml`) lives one or more directories
/// deep, so a single non-recursive `read_dir` would silently miss it and
/// let the pack's colliding entry go unshadowed.
fn walk_yaml_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_yaml_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "yaml" || ext == "yml")
        {
            out.push(path);
        }
    }
}

fn pack_catalog_entries(
    cwd: &std::path::Path,
) -> (Vec<CatalogEntry>, Vec<yunta_engine::CatalogError>) {
    let mut entries = Vec::new();
    let mut broken = Vec::new();
    for publisher in yunta_engine::installed_publishers(cwd) {
        let packs = yunta_engine::packs_for_publisher(cwd, &publisher);
        for (pack_dir, manifest) in packs.installed {
            for declared in &manifest.contents.workflows {
                let Some(stem) = std::path::Path::new(declared)
                    .file_stem()
                    .and_then(|s| s.to_str())
                else {
                    continue;
                };
                entries.push(CatalogEntry {
                    display_name: format!("{publisher}/{stem}"),
                    path: pack_dir.join(declared),
                });
            }
        }
        broken.extend(packs.broken);
    }
    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    (entries, broken)
}
