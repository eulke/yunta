//! `yunta list`: the repo's own catalog of workflows — name,
//! description, declared inputs — with `--runs` switching to local runs
//! and their derived state instead. Both answer the same question
//! without a server: "what's here, and where does it stand."
//!
//! The catalog is two layers: the repo's own `.yunta/workflows/`, then
//! every installed pack's declared `contents.workflows`, addressed
//! `publisher/name` — a bare repo name never collides with a pack entry
//! since the two are printed and looked up under different keys.

use std::path::PathBuf;
use std::process::ExitCode;

use yunta_core::{InputSpec, Manifest, Workflow};

use super::status::progress_summary;
use crate::project;

/// One catalog entry ready to render — `display_name` already carries
/// the `publisher/` prefix for a pack entry, nothing else needs to know
/// where it came from.
struct CatalogEntry {
    display_name: String,
    path: PathBuf,
}

pub fn list_workflows() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Best-effort — a project with no state root yet (never ran
    // anything) simply shows no estimation, same as "fewer than three
    // runs" does; neither is an error worth refusing the catalog over.
    let history_source = project::resolve(&cwd).ok().and_then(|project| {
        yunta_storage::Storage::open(&project.storage_path)
            .ok()
            .map(|storage| (project, storage))
    });

    let mut entries = repo_catalog_entries(&cwd);
    let shadowed: std::collections::HashSet<String> =
        entries.iter().map(|e| e.display_name.clone()).collect();
    entries.extend(
        pack_catalog_entries(&cwd)
            .into_iter()
            .filter(|e| !shadowed.contains(&e.display_name)),
    );

    if entries.is_empty() {
        println!(
            "no workflows under {} or {}",
            cwd.join(".yunta/workflows").display(),
            cwd.join(".yunta/packs").display()
        );
        return ExitCode::SUCCESS;
    }

    for entry in entries {
        let name = &entry.display_name;
        let contents = match std::fs::read_to_string(&entry.path) {
            Ok(c) => c,
            Err(e) => {
                println!("{name}: unreadable ({e})");
                continue;
            }
        };
        let workflow: Workflow = match yunta_core::yaml::parse(&contents) {
            Ok(w) => w,
            Err(e) => {
                println!("{name}: fails to parse ({e})");
                continue;
            }
        };
        println!(
            "{name}: {}",
            workflow
                .description
                .as_deref()
                .unwrap_or("(no description)")
        );
        for (input_name, spec) in &workflow.inputs {
            let optionality = if spec.is_required() {
                "required"
            } else {
                "optional"
            };
            let description = spec.description().unwrap_or("");
            println!(
                "  --input {input_name}=... ({}, {optionality}){}",
                input_type_label(spec),
                if description.is_empty() {
                    String::new()
                } else {
                    format!(" — {description}")
                }
            );
        }
        if let Some((project, storage)) = &history_source {
            let history =
                super::stats::collect_history(&project.runs_root, storage, &workflow.name);
            if let Some(estimation) = yunta_engine::prior_estimation(&history) {
                println!("  {}", super::stats::format_estimation_line(&estimation));
            }
        }
    }
    ExitCode::SUCCESS
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

fn pack_catalog_entries(cwd: &std::path::Path) -> Vec<CatalogEntry> {
    let mut entries = Vec::new();
    for publisher in yunta_engine::installed_publishers(cwd) {
        for (pack_dir, manifest) in yunta_engine::packs_for_publisher(cwd, &publisher) {
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
    }
    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    entries
}

fn input_type_label(spec: &InputSpec) -> &'static str {
    match spec {
        InputSpec::String { .. } => "string",
        InputSpec::Number { .. } => "number",
        InputSpec::Boolean { .. } => "boolean",
        InputSpec::Enum { .. } => "enum",
        InputSpec::Path { .. } => "path",
    }
}

pub fn list_runs() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let project = match project::resolve(&cwd) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let storage = match yunta_storage::Storage::open(&project.storage_path) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let run_ids = match storage
        .list_runs()
        .map(|runs| runs.into_iter().map(|run| run.run_id).collect::<Vec<_>>())
    {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if run_ids.is_empty() {
        println!("no runs in {}", project.storage_path.display());
        return ExitCode::SUCCESS;
    }

    for run_id in run_ids {
        let events = match storage.events_for_run(&run_id) {
            Ok(events) => events,
            Err(e) => {
                println!("{run_id}: unreadable ({e})");
                continue;
            }
        };
        let manifest_path = project
            .runs_root
            .join(run_id.as_str())
            .join("manifest.yaml");
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
    ExitCode::SUCCESS
}
