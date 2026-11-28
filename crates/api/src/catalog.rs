//! See [`crate`]. The catalog: the workflows the repository a workspace works
//! in offers. It is two layers: the repository's own `.yunta/workflows/`, then
//! every installed pack's declared `contents.workflows`, addressed
//! `publisher/name`. A repository's own entry shadows a pack's of the same
//! name, so a bare repository name never collides with a pack entry. A setup
//! reads the workflows the catalog would offer once a pack it fetched is
//! installed, the fetched copy's among the installed packs' in the same order,
//! so what it proposes is what `init` proposes after the install; the fetched
//! copy's are read through the tree its fetch walked, never through a link.
//!
//! What the catalog offers is read off the repository alone; each workflow's
//! past runs are read off the project's history when the project resolves,
//! once a workflow, and kept with it: how often it ran here, how its runs
//! usually went and what each spent are all worked out from that one reading,
//! so no reader of the catalog reads a history again. A project that does not
//! resolve reads no history and shows none; one with fewer runs than an
//! estimate needs shows how many it holds and no estimate. The same
//! resolution's config says which adapters a run here can be pinned to, so a
//! new run offers what admission would take without reading the config again.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use yunta_core::{PackManifest, Publisher, Workflow};
use yunta_render::page::{Catalog, OfferedWorkflow, ReadableWorkflow, UnreadableWorkflow};

use crate::{FetchedPack, Project, Workspace};

/// One workflow the catalog names, before its file is read: the name a person
/// runs it with — `publisher/` first for a pack's — and where it is.
struct CatalogEntry {
    name: String,
    path: PathBuf,
}

impl Workspace {
    /// What the catalog offers here: each workflow with how its past runs
    /// usually went, or why its file does not read; the packs that do not
    /// read; where workflows and packs are declared; and the adapters a run
    /// of them can be pinned to, from the config the project resolved with,
    /// the one admission freezes.
    pub async fn catalog(&self) -> Catalog {
        let project = self.project().ok();
        let mut entries = repository_entries(&self.dir);
        let shadowed: HashSet<String> = entries.iter().map(|entry| entry.name.clone()).collect();
        let (pack_entries, broken_packs) = pack_entries(&self.dir);
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
            packs_dir: self.dir.join(".yunta/packs"),
            offered,
            pinnable: project
                .map(|project| crate::machine::pinnable_adapter_ids(&project.config))
                .unwrap_or_default(),
        }
    }

    /// Every workflow the catalog offers that reads back — the repository's
    /// own, then every installed pack's — by the name a person runs it with.
    /// What does not read is left to `list` and `check` to name.
    pub fn catalog_workflows(&self) -> Vec<(String, Workflow)> {
        let repository = repository_entries(&self.dir).into_iter().map(on_disk);
        let packs = pack_entries(&self.dir).0.into_iter().map(on_disk);
        read_back(repository.chain(packs))
    }

    /// Every workflow the catalog would offer that reads back once `pack`, fetched,
    /// is installed: the repository's own, then every pack's, the fetched one's
    /// among the installed ones' in the order the catalog lists them, each read from
    /// the copy fetched, through the tree its fetch walked. Nothing of the pack is
    /// written to list it, nor read from outside its tree.
    pub(crate) fn catalog_workflows_installing(
        &self,
        pack: &FetchedPack,
    ) -> Vec<(String, Workflow)> {
        let (installed, _) = pack_entries(&self.dir);
        let mut packs: Vec<Text> = installed.into_iter().map(on_disk).collect();
        let (manifest, tree) = (&pack.manifest, pack.tree());
        let fetched = declared_entries(&manifest.publisher, tree.root(), manifest);
        packs.extend(fetched.into_iter().map(|entry| {
            let in_the_pack = entry.path.strip_prefix(tree.root()).unwrap_or(&entry.path);
            let text = tree.read(in_the_pack).ok();
            (entry, text)
        }));
        packs.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));
        let repository = repository_entries(&self.dir).into_iter().map(on_disk);
        read_back(repository.chain(packs))
    }
}

/// A workflow the catalog names, and its file's text, none where it does not read.
type Text = (CatalogEntry, Option<String>);

/// `entry` with its file's text, read from the disk.
fn on_disk(entry: CatalogEntry) -> Text {
    let text = std::fs::read_to_string(&entry.path).ok();
    (entry, text)
}

/// The workflows of `entries` that read back, in their order, by the name a person
/// runs each with.
fn read_back(entries: impl Iterator<Item = Text>) -> Vec<(String, Workflow)> {
    entries
        .filter_map(|(entry, text)| {
            let workflow = yunta_core::workflow::read::read(&text?, &entry.path).ok()?;
            Some((entry.name, workflow))
        })
        .collect()
}

/// `entry` as the catalog offers it: its workflow and its past runs in
/// `project`, or why its file does not read.
async fn offer(entry: CatalogEntry, project: Option<&Project>) -> OfferedWorkflow {
    let workflow = match read(&entry).await {
        Ok(workflow) => {
            let past = match project {
                Some(project) => Some(project.history(&workflow.name).await.summaries()),
                None => None,
            };
            Ok(ReadableWorkflow { workflow, past })
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

/// Every workflow the installed packs under `dir` declare, by
/// `publisher/name`, and the packs that do not read.
fn pack_entries(dir: &Path) -> (Vec<CatalogEntry>, Vec<yunta_engine::CatalogError>) {
    let mut entries = Vec::new();
    let mut broken = Vec::new();
    for publisher in yunta_engine::installed_publishers(dir) {
        let packs = yunta_engine::packs_for_publisher(dir, &publisher);
        for (pack_dir, manifest) in packs.installed {
            entries.extend(declared_entries(&publisher, &pack_dir, &manifest));
        }
        broken.extend(packs.broken);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    (entries, broken)
}

/// Every workflow `manifest` declares, its pack's files standing at `pack_dir`, by
/// `publisher/name`.
fn declared_entries(
    publisher: &Publisher,
    pack_dir: &Path,
    manifest: &PackManifest,
) -> Vec<CatalogEntry> {
    let named = |declared: &String| {
        let stem = Path::new(declared).file_stem()?.to_str()?;
        Some(CatalogEntry {
            name: format!("{publisher}/{stem}"),
            path: pack_dir.join(declared),
        })
    };
    manifest
        .contents
        .workflows
        .iter()
        .filter_map(named)
        .collect()
}
