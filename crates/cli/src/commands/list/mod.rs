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

use yunta_core::{InputSpec, Workflow};

use crate::commands::advice;
use crate::context::Context;
use crate::error::{CliError, Outcome};
use crate::output::document;
use surco::components::{Fields, Gist, Headline, Marked, Next, Prose, Section};
use surco::doc::{Block, Depth, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;

pub use runs::list_runs;

/// `yunta` alone: in a repository, its field — the runs of that
/// repository; anywhere else, where Yunta works and what to type.
pub async fn field(look: &Look) -> Result<Outcome, CliError> {
    let ctx = Context::load()?;
    if inbox::in_a_repository(&ctx).await? {
        return list_runs(false, Depth::Glance, crate::output::Mode::Human, look).await;
    }
    let page = Doc::new()
        .with(Headline::new(Mark::Absent, "yunta", "not in a repository"))
        .with(Gist::new(
            "this directory is in no git repository, and yunta works in one",
        ))
        .with(Next {
            steps: vec![
                (advice::list_every_run(), "every run on this machine"),
                (advice::help(), "what yunta does"),
            ],
        });
    document(look, page);
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
    let catalog = catalog(&cwd, Context::load().ok().as_ref(), look.glyphs).await;
    document(look, catalog);
    Ok(Outcome::Success)
}

/// The repo catalog — the repo's own `.yunta/workflows/` plus every
/// installed pack's, a pack entry hidden behind a repo file of the same
/// `publisher/name` — as one document: `yunta list` draws it on the
/// terminal and the `list_workflows` control-plane tool answers with it
/// as Markdown, so the two never drift. Given a project's storage, each
/// workflow carries its prior estimation; a broken pack is named, never
/// silently dropped. `glyphs` draws each input's placeholder and the
/// separators of an estimation.
pub(crate) async fn catalog(
    cwd: &Path,
    history_source: Option<&Context>,
    glyphs: surco::glyphs::Glyphs,
) -> Doc {
    let mut entries = repo_catalog_entries(cwd);
    let shadowed: HashSet<String> = entries.iter().map(|e| e.display_name.clone()).collect();
    let (pack_entries, broken_packs) = pack_catalog_entries(cwd);
    let mut doc = Doc::new();
    if !broken_packs.is_empty() {
        doc = doc.with(Marked {
            mark: Mark::Failed,
            items: broken_packs.iter().map(ToString::to_string).collect(),
        });
    }
    entries.extend(
        pack_entries
            .into_iter()
            .filter(|e| !shadowed.contains(&e.display_name)),
    );
    if entries.is_empty() {
        if broken_packs.is_empty() {
            doc = doc.with(Prose(format!(
                "no workflows under {} or {}",
                cwd.join(".yunta/workflows").display(),
                cwd.join(".yunta/packs").display()
            )));
        }
        return doc;
    }
    for entry in entries {
        doc = doc.with(listed(&entry, history_source, glyphs).await);
    }
    doc
}

/// One workflow the catalog offers: its name and what it does, the
/// inputs it takes, how its past runs went, and the command that runs it
/// — or why it does not read.
async fn listed(
    entry: &CatalogEntry,
    history_source: Option<&Context>,
    glyphs: surco::glyphs::Glyphs,
) -> Section {
    let name = &entry.display_name;
    let workflow = match read_entry(entry).await {
        Ok(workflow) => workflow,
        Err(why) => {
            return Section {
                lead: None,
                title: Line::new().push(Tone::Strong, name.as_str()),
                blocks: why,
            }
        }
    };
    let mut title = Line::new().push(Tone::Strong, name.as_str());
    if let Some(description) = workflow.description.as_deref() {
        title = title.plain(format!(": {description}"));
    }
    let mut fields = input_fields(&workflow, glyphs);
    if let Some(ctx) = history_source {
        fields = estimation_field(fields, ctx, &workflow, glyphs).await;
    }
    fields = fields.push_command("run", advice::run(name));
    Section {
        lead: None,
        title,
        blocks: vec![fields.into()],
    }
}

/// The workflow `entry` names, or the reason its file does not read.
async fn read_entry(entry: &CatalogEntry) -> Result<Workflow, Vec<Block>> {
    let contents = tokio::fs::read_to_string(&entry.path).await.map_err(|e| {
        vec![Marked {
            mark: Mark::Failed,
            items: vec![format!("unreadable: {e}")],
        }
        .into()]
    })?;
    yunta_core::workflow::read::read(&contents, &entry.path).map_err(|report| {
        crate::error::page::document_problems(&report, Some(&contents))
            .into_iter()
            .map(Block::from)
            .collect()
    })
}

/// One row per input the workflow takes, the first one carrying the label.
/// Each shows its value as a placeholder, which is the look's ellipsis: three
/// dots under the ASCII look.
fn input_fields(workflow: &Workflow, glyphs: surco::glyphs::Glyphs) -> Fields {
    let mut fields = Fields::new();
    for (n, (input_name, spec)) in workflow.inputs.iter().enumerate() {
        let optionality = if spec.is_required() {
            "required"
        } else {
            "optional"
        };
        let description = spec
            .description()
            .map(|description| format!(": {description}"))
            .unwrap_or_default();
        fields = fields.push_if(
            if n == 0 { "inputs" } else { "" },
            format!(
                "--input {input_name}={} ({}, {optionality}){description}",
                glyphs.ellipsis(),
                input_type_label(spec)
            ),
        );
    }
    fields
}

/// The `usually` row of the workflow's prior estimation, when its history has one.
async fn estimation_field(
    mut fields: Fields,
    ctx: &Context,
    workflow: &Workflow,
    glyphs: surco::glyphs::Glyphs,
) -> Fields {
    let history = super::stats::summaries(&super::stats::history(ctx, &workflow.name).await);
    if let Some(estimation) = yunta_replay::history::PriorEstimation::of(&history) {
        fields = fields.push_if(
            "usually",
            super::stats::format_estimation_line(&estimation, glyphs),
        );
    }
    fields
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
