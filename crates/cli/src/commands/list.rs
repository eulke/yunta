//! `yunta list` (T7.1): the repo's own catalog of workflows — name,
//! description, declared inputs — with `--runs` switching to local runs
//! and their derived state instead. Both answer the same question
//! without a server: "what's here, and where does it stand."
//!
//! Packs (RFC-0002) would add to the workflow catalog once M11 exists;
//! this recorte only ever looks under `.yunta/workflows/`, same root
//! `yunta test` already resolves case workflows from.

use std::process::ExitCode;

use yunta_core::{InputSpec, Manifest, Workflow};

use super::status::progress_summary;
use crate::project;

pub fn list_workflows() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    // §8.6: best-effort — a project with no state root yet (never ran
    // anything) simply shows no estimation, same as "fewer than three
    // runs" does; neither is an error worth refusing the catalog over.
    let history_source = project::resolve(&cwd).ok().and_then(|project| {
        yunta_storage::Storage::open(&project.storage_path)
            .ok()
            .map(|storage| (project, storage))
    });

    let workflows_dir = cwd.join(".yunta/workflows");
    let entries = match std::fs::read_dir(&workflows_dir) {
        Ok(entries) => entries,
        Err(_) => {
            println!("no workflows under {}", workflows_dir.display());
            return ExitCode::SUCCESS;
        }
    };

    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    paths.sort();

    if paths.is_empty() {
        println!("no workflows under {}", workflows_dir.display());
        return ExitCode::SUCCESS;
    }

    for path in paths {
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string());
        let name = name.as_deref().unwrap_or("?");
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                println!("{name}: unreadable ({e})");
                continue;
            }
        };
        let workflow: Workflow = match serde_yaml::from_str(&contents) {
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
            let optionality = if spec.has_default() {
                "optional"
            } else {
                "required"
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
            let history = super::stats::collect_history(project, storage, &workflow.name);
            if let Some(estimation) = yunta_engine::prior_estimation(&history) {
                println!("  {}", super::stats::format_estimation_line(&estimation));
            }
        }
    }
    ExitCode::SUCCESS
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

    let run_ids = match storage.list_run_ids() {
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
            .and_then(|c| serde_yaml::from_str::<Manifest>(&c).ok())
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
