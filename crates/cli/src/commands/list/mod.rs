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

use yunta_core::step::{Arg, NextStep, Reach};
use yunta_core::{InputSpec, Workflow};
use yunta_render::command;
use yunta_render::page::Page;

use crate::context::Context;
use crate::error::CliError;
use crate::output::show;
use crate::output::Outcome;
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
    show(look, &Signpost);
    Ok(Outcome::Success)
}

/// What `yunta` alone shows where it has no field to show: that it works in
/// a repository, and the two ways on from here.
struct Signpost;

impl Page for Signpost {
    fn document(&self, _look: &Look) -> Doc {
        Doc::new()
            .with(Headline::new(Mark::Absent, "yunta", "not in a repository"))
            .with(Gist::new(
                "this directory is in no git repository, and yunta works in one",
            ))
            .with(Next {
                steps: vec![
                    (
                        command::line(&NextStep::ListRuns {
                            reach: Reach::Machine,
                        }),
                        "every run on this machine",
                    ),
                    (command::line(&NextStep::Help), "what yunta does"),
                ],
            })
    }
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
    let catalog = Catalog::read(&cwd, Context::load().ok().as_ref()).await;
    show(look, &catalog);
    Ok(Outcome::Success)
}

/// The repo catalog — the repo's own `.yunta/workflows/` plus every
/// installed pack's, a pack entry hidden behind a repo file of the same
/// `publisher/name` — as one page: `yunta list` draws it on the terminal and
/// the `list_workflows` control-plane tool answers with it as Markdown, so
/// the two never drift. Given a project's storage, each workflow carries its
/// prior estimation; a broken pack is named, never silently dropped.
pub(crate) struct Catalog {
    /// Each installed pack that does not read, as its error says it.
    broken_packs: Vec<String>,
    /// Where the catalog looked: the repo's workflows, then its packs.
    looked_in: [PathBuf; 2],
    offered: Vec<Offered>,
}

/// One workflow the catalog offers, by the name a person runs it with, as
/// its file read.
struct Offered {
    name: String,
    workflow: Result<Readable, Unreadable>,
}

/// A workflow that reads, and how its past runs usually went, when its
/// history says.
struct Readable {
    workflow: Workflow,
    usually: Option<yunta_replay::history::PriorEstimation>,
}

/// Why a workflow file the catalog offers does not read.
enum Unreadable {
    /// The file could not be read, as the error says.
    File(String),
    /// It reads, but not as a workflow: each problem, quoted from it.
    NotAWorkflow(Vec<surco::components::Problem>),
}

impl Catalog {
    /// What the catalog offers under `cwd`, with each workflow's history read
    /// from `history_source` when there is one.
    pub(crate) async fn read(cwd: &Path, history_source: Option<&Context>) -> Catalog {
        let mut entries = repo_catalog_entries(cwd);
        let shadowed: HashSet<String> = entries.iter().map(|e| e.display_name.clone()).collect();
        let (pack_entries, broken_packs) = pack_catalog_entries(cwd);
        entries.extend(
            pack_entries
                .into_iter()
                .filter(|e| !shadowed.contains(&e.display_name)),
        );
        let mut offered = Vec::new();
        for entry in entries {
            offered.push(Offered::read(entry, history_source).await);
        }
        Catalog {
            broken_packs: broken_packs.iter().map(ToString::to_string).collect(),
            looked_in: [cwd.join(".yunta/workflows"), cwd.join(".yunta/packs")],
            offered,
        }
    }
}

impl Page for Catalog {
    /// Each broken pack, then each workflow; where there is neither, where
    /// the catalog looked. The look's glyphs draw each input's placeholder
    /// and the separators of an estimation.
    fn document(&self, look: &Look) -> Doc {
        let mut doc = Doc::new();
        if !self.broken_packs.is_empty() {
            doc = doc.with(Marked {
                mark: Mark::Failed,
                items: self.broken_packs.clone(),
            });
        }
        if self.offered.is_empty() {
            if self.broken_packs.is_empty() {
                let [workflows, packs] = &self.looked_in;
                doc = doc.with(Prose(format!(
                    "no workflows under {} or {}",
                    workflows.display(),
                    packs.display()
                )));
            }
            return doc;
        }
        self.offered
            .iter()
            .fold(doc, |doc, offered| doc.with(offered.section(look.glyphs)))
    }
}

impl Offered {
    /// `entry` as the catalog offers it: its workflow and how its past runs
    /// usually went, or why its file does not read.
    async fn read(entry: CatalogEntry, history_source: Option<&Context>) -> Offered {
        let workflow = match read_entry(&entry).await {
            Ok(workflow) => {
                let usually = match history_source {
                    Some(ctx) => estimation(ctx, &workflow).await,
                    None => None,
                };
                Ok(Readable { workflow, usually })
            }
            Err(why) => Err(why),
        };
        Offered {
            name: entry.display_name,
            workflow,
        }
    }

    /// One workflow the catalog offers: its name and what it does, the
    /// inputs it takes, how its past runs went, and the command that runs it
    /// — or why it does not read.
    fn section(&self, glyphs: surco::glyphs::Glyphs) -> Section {
        let name = self.name.as_str();
        let readable = match &self.workflow {
            Ok(readable) => readable,
            Err(why) => {
                return Section {
                    lead: None,
                    title: Line::new().push(Tone::Strong, name),
                    blocks: why.blocks(),
                }
            }
        };
        let workflow = &readable.workflow;
        let mut title = Line::new().push(Tone::Strong, name);
        if let Some(description) = workflow.description.as_deref() {
            title = title.plain(format!(": {description}"));
        }
        let mut fields = input_fields(workflow, glyphs);
        if let Some(estimation) = &readable.usually {
            fields = fields.push_if(
                "usually",
                super::stats::format_estimation_line(estimation, glyphs),
            );
        }
        fields = fields.push_command(
            "run",
            command::line(&NextStep::Run {
                workflow: Arg::Given(name.to_string()),
            }),
        );
        Section {
            lead: None,
            title,
            blocks: vec![fields.into()],
        }
    }
}

impl Unreadable {
    /// What a section says of a file that does not read.
    fn blocks(&self) -> Vec<Block> {
        match self {
            Unreadable::File(error) => vec![Marked {
                mark: Mark::Failed,
                items: vec![format!("unreadable: {error}")],
            }
            .into()],
            Unreadable::NotAWorkflow(problems) => {
                problems.iter().cloned().map(Block::from).collect()
            }
        }
    }
}

/// The workflow `entry` names, or the reason its file does not read.
async fn read_entry(entry: &CatalogEntry) -> Result<Workflow, Unreadable> {
    let contents = tokio::fs::read_to_string(&entry.path)
        .await
        .map_err(|e| Unreadable::File(e.to_string()))?;
    yunta_core::workflow::read::read(&contents, &entry.path).map_err(|report| {
        Unreadable::NotAWorkflow(crate::error::page::document_problems(
            &report,
            Some(&contents),
        ))
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
