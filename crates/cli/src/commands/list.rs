//! `yunta list`: the repo's own catalog of workflows — name,
//! description, declared inputs — with `--runs` switching to local runs
//! and their derived state instead. Both answer the same question
//! without a server: "what's here, and where does it stand."
//!
//! The catalog is two layers: the repo's own `.yunta/workflows/`, then
//! every installed pack's declared `contents.workflows`, addressed
//! `publisher/name` — a bare repo name never collides with a pack entry
//! since the two are printed and looked up under different keys.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use yunta_core::{InputSpec, Manifest, Workflow};
use yunta_storage::Storage;

use super::status::progress_summary;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::project::Project;

/// One catalog entry ready to render — `display_name` already carries
/// the `publisher/` prefix for a pack entry, nothing else needs to know
/// where it came from.
struct CatalogEntry {
    display_name: String,
    path: PathBuf,
}

pub fn list_workflows() -> Result<Outcome, CliError> {
    let cwd = std::env::current_dir().map_err(|source| CliError::Cwd { source })?;

    // Best-effort — a project with no state root yet (never ran
    // anything) simply shows no estimation, same as "fewer than three
    // runs" does; neither is an error worth refusing the catalog over.
    let history_source = Context::load().ok().and_then(|ctx| {
        let storage = ctx.storage().ok()?;
        Some((ctx.project, storage))
    });

    print!("{}", render_catalog(&cwd, history_source.as_ref()));
    Ok(Outcome::Success)
}

/// Renders the repo catalog — the repo's own `.yunta/workflows/` plus
/// every installed pack's declared workflows, a pack entry shadowed by a
/// repo file of the same `publisher/name` — as the text both `yunta list`
/// prints and the `list_workflows` control-plane tool returns, so the two
/// never drift. Given a project's storage, each workflow carries its prior
/// estimation; a broken pack is named, never silently dropped.
pub(crate) fn render_catalog(cwd: &Path, history_source: Option<&(Project, Storage)>) -> String {
    let mut out = String::new();
    let mut entries = repo_catalog_entries(cwd);
    let shadowed: HashSet<String> = entries.iter().map(|e| e.display_name.clone()).collect();
    let (pack_entries, broken_packs) = pack_catalog_entries(cwd);
    for err in &broken_packs {
        out.push_str(&format!("{err}\n"));
    }
    entries.extend(
        pack_entries
            .into_iter()
            .filter(|e| !shadowed.contains(&e.display_name)),
    );

    if entries.is_empty() {
        if broken_packs.is_empty() {
            out.push_str(&format!(
                "no workflows under {} or {}\n",
                cwd.join(".yunta/workflows").display(),
                cwd.join(".yunta/packs").display()
            ));
        }
        return out;
    }

    for entry in entries {
        let name = &entry.display_name;
        let contents = match std::fs::read_to_string(&entry.path) {
            Ok(c) => c,
            Err(e) => {
                out.push_str(&format!("{name}: unreadable ({e})\n"));
                continue;
            }
        };
        let workflow: Workflow = match yunta_core::yaml::parse(&contents) {
            Ok(w) => w,
            Err(e) => {
                out.push_str(&format!("{name}: fails to parse ({e})\n"));
                continue;
            }
        };
        out.push_str(&format!(
            "{name}: {}\n",
            workflow
                .description
                .as_deref()
                .unwrap_or("(no description)")
        ));
        for (input_name, spec) in &workflow.inputs {
            let optionality = if spec.is_required() {
                "required"
            } else {
                "optional"
            };
            let description = spec.description().unwrap_or("");
            out.push_str(&format!(
                "  --input {input_name}=... ({}, {optionality}){}\n",
                input_type_label(spec),
                if description.is_empty() {
                    String::new()
                } else {
                    format!(" — {description}")
                }
            ));
        }
        if let Some((project, storage)) = history_source {
            let history =
                super::stats::collect_history(&project.runs_root, storage, &workflow.name);
            if let Some(estimation) = yunta_engine::prior_estimation(&history) {
                out.push_str(&format!(
                    "  {}\n",
                    super::stats::format_estimation_line(&estimation)
                ));
            }
        }
    }
    out
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

fn input_type_label(spec: &InputSpec) -> &'static str {
    match spec {
        InputSpec::String { .. } => "string",
        InputSpec::Number { .. } => "number",
        InputSpec::Boolean { .. } => "boolean",
        InputSpec::Enum { .. } => "enum",
        InputSpec::Path { .. } => "path",
        InputSpec::Document { .. } => "document",
    }
}

pub fn list_runs() -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    let storage = ctx.storage()?;

    let run_ids = storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect::<Vec<_>>())?;
    if run_ids.is_empty() {
        println!("no runs in {}", ctx.project.storage_path.display());
        return Ok(Outcome::Success);
    }

    for run_id in run_ids {
        let events = match storage.events_for_run(&run_id) {
            Ok(events) => events,
            Err(e) => {
                println!("{run_id}: unreadable ({e})");
                continue;
            }
        };
        // The same frozen-path-aware search `status` uses, so a run
        // created under a since-changed `paths.runs` still lists.
        let Some(run_dir) = ctx.project.run_dir(run_id.as_str()) else {
            println!(
                "{run_id}: manifest missing or unreadable under {}",
                ctx.project.runs_root.display()
            );
            continue;
        };
        let manifest_path = run_dir.join("manifest.yaml");
        let manifest = match std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|c| yunta_core::yaml::parse::<Manifest>(&c).ok())
        {
            Some(manifest) => manifest,
            None => {
                println!(
                    "{run_id}: manifest missing or unreadable at {}",
                    manifest_path.display()
                );
                continue;
            }
        };
        println!("{run_id}: {}", progress_summary(&events, &manifest));
    }
    Ok(Outcome::Success)
}
