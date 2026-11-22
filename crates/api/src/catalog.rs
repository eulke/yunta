//! See [`crate`]. The catalog: the workflows the repository a workspace works
//! in offers. It is two layers: the repository's own `.yunta/workflows/`, then
//! the declared `contents.workflows` of every pack the project reads, its own
//! and the user's, addressed `publisher/name`. A repository's own entry
//! shadows a pack's of the same name, so a bare repository name never
//! collides with a pack entry, and a pack name is listed from the store that
//! answers it, as a run resolves it.
//!
//! What the catalog offers is read off the repository alone; how each
//! workflow's past runs usually went is read off the project's history when
//! the project resolves, and a project with no state root yet, like one with
//! fewer runs than an estimate needs, simply shows none.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use yunta_core::{Env, PackStore, Workflow};
use yunta_render::page::{Catalog, OfferedWorkflow, ReadableWorkflow, UnreadableWorkflow};

use crate::{Project, Workspace};

/// One workflow the catalog names, before its file is read: the name a person
/// runs it with — `publisher/` first for a pack's — and where it is.
struct CatalogEntry {
    name: String,
    path: PathBuf,
}

impl Workspace {
    /// What the catalog offers here: each workflow with how its past runs
    /// usually went, or why its file does not read; the packs that do not
    /// read; and where workflows and packs are declared.
    pub async fn catalog(&self) -> Catalog {
        let project = self.project().ok();
        let mut entries = repository_entries(&self.dir);
        let shadowed: HashSet<String> = entries.iter().map(|entry| entry.name.clone()).collect();
        let (pack_entries, broken_packs) = pack_entries(&self.dir, &self.env);
        entries.extend(
            pack_entries
                .into_iter()
                .filter(|entry| !shadowed.contains(&entry.name)),
        );
        let mut offered = Vec::new();
        for entry in entries {
            offered.push(offer(entry, project.as_ref()).await);
        }
        Catalog {
            broken_packs: broken_packs.iter().map(ToString::to_string).collect(),
            workflows_dir: self.dir.join(".yunta/workflows"),
            packs_dir: PackStore::of_project(&self.dir).packs_dir(),
            user_packs_dir: PackStore::of_user(&self.env).map(|store| store.packs_dir()),
            offered,
        }
    }

    /// Every workflow the catalog offers that reads back — the repository's
    /// own, then every installed pack's — by the name a person runs it with.
    /// What does not read is left to `list` and `check` to name.
    pub fn catalog_workflows(&self) -> Vec<(String, Workflow)> {
        repository_entries(&self.dir)
            .into_iter()
            .chain(pack_entries(&self.dir, &self.env).0)
            .filter_map(|entry| {
                let contents = std::fs::read_to_string(&entry.path).ok()?;
                let workflow = yunta_core::workflow::read::read(&contents, &entry.path).ok()?;
                Some((entry.name, workflow))
            })
            .collect()
    }
}

/// `entry` as the catalog offers it: its workflow and how its past runs
/// usually went in `project`, or why its file does not read.
async fn offer(entry: CatalogEntry, project: Option<&Project>) -> OfferedWorkflow {
    let workflow = match read(&entry).await {
        Ok(workflow) => {
            let usually = match project {
                Some(project) => {
                    let history = project.history(&workflow.name).await;
                    yunta_replay::history::PriorEstimation::of(&history.summaries())
                }
                None => None,
            };
            Ok(ReadableWorkflow { workflow, usually })
        }
        Err(why) => Err(why),
    };
    OfferedWorkflow {
        name: entry.name,
        workflow,
    }
}

/// The workflow `entry` names, or the reason its file does not read.
async fn read(entry: &CatalogEntry) -> Result<Workflow, UnreadableWorkflow> {
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

/// The repository's own workflows under `dir`, named by their path under
/// `.yunta/workflows/` without the extension.
fn repository_entries(dir: &Path) -> Vec<CatalogEntry> {
    let workflows_dir = dir.join(".yunta/workflows");
    let mut paths = Vec::new();
    walk_yaml_files(&workflows_dir, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(&workflows_dir)
                .unwrap_or(&path)
                .with_extension("")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            CatalogEntry { name, path }
        })
        .collect()
}

/// Recursively collects every `.yaml`/`.yml` file under `dir` — a repo
/// workflow that shadows a namespaced pack entry (e.g.
/// `.yunta/workflows/acme/review.yaml`) lives one or more directories
/// deep, so a single non-recursive `read_dir` would silently miss it and
/// let the pack's colliding entry go unshadowed.
fn walk_yaml_files(dir: &Path, out: &mut Vec<PathBuf>) {
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

/// Every workflow the packs the project in `dir` reads declare, by
/// `publisher/name`, each from the store that answers its name, and the
/// packs that do not read. `env` names the user's store.
fn pack_entries(dir: &Path, env: &Env) -> (Vec<CatalogEntry>, Vec<yunta_engine::CatalogError>) {
    let mut entries = Vec::new();
    let mut broken = Vec::new();
    for publisher in yunta_engine::installed_publishers(dir, Some(env)) {
        let packs = yunta_engine::packs_for_publisher(dir, Some(env), &publisher);
        entries.extend(
            packs
                .workflows()
                .into_iter()
                .map(|(stem, path)| CatalogEntry {
                    name: format!("{publisher}/{stem}"),
                    path,
                }),
        );
        broken.extend(packs.broken);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    (entries, broken)
}
