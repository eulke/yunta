//! The repo catalog, as a person reads it: every workflow the repository and
//! its installed packs offer, by the name a person runs it with — what it
//! does, the inputs it takes, how its past runs went and the command that
//! runs it, or why its file does not read — and each pack that does not
//! read. `yunta list` draws it on a terminal, and the control plane's
//! `list_workflows` tool answers with it as Markdown, so the two never
//! drift. Reading it is its caller's.

use std::path::PathBuf;

use surco::components::{Fields, Marked, Prose, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::step::{Arg, NextStep};
use yunta_core::{InputSpec, Workflow};

use super::Page;
use crate::command;

/// The repo catalog — the repo's own `.yunta/workflows/` plus every
/// installed pack's, a pack entry hidden behind a repo file of the same
/// `publisher/name` — as one page: `yunta list` draws it on the terminal and
/// the `list_workflows` control-plane tool answers with it as Markdown, so
/// the two never drift. Given a project's storage, each workflow carries its
/// prior estimation; a broken pack is named, never silently dropped.
pub struct Catalog {
    /// Each installed pack that does not read, as its error says it.
    pub broken_packs: Vec<String>,
    /// Where the catalog looked for the repository's own workflows.
    pub workflows_dir: PathBuf,
    /// Where it looked for the packs installed in it.
    pub packs_dir: PathBuf,
    pub offered: Vec<OfferedWorkflow>,
}

/// One workflow the catalog offers, by the name a person runs it with, as
/// its file read.
pub struct OfferedWorkflow {
    pub name: String,
    pub workflow: Result<ReadableWorkflow, UnreadableWorkflow>,
}

/// A workflow that reads, and how its past runs usually went, when its
/// history says.
pub struct ReadableWorkflow {
    pub workflow: Workflow,
    pub usually: Option<yunta_replay::history::PriorEstimation>,
}

/// Why a workflow file the catalog offers does not read.
pub enum UnreadableWorkflow {
    /// The file could not be read, as the error says.
    File(String),
    /// It reads, but not as a workflow: each problem, quoted from it.
    NotAWorkflow(Vec<surco::components::Problem>),
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
                doc = doc.with(Prose(format!(
                    "no workflows under {} or {}",
                    self.workflows_dir.display(),
                    self.packs_dir.display()
                )));
            }
            return doc;
        }
        self.offered
            .iter()
            .fold(doc, |doc, offered| doc.with(offered.section(look.glyphs)))
    }
}

impl OfferedWorkflow {
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
            fields = fields.push_if("usually", crate::spend::estimation(estimation, glyphs));
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

impl UnreadableWorkflow {
    /// What a section says of a file that does not read.
    fn blocks(&self) -> Vec<Block> {
        match self {
            UnreadableWorkflow::File(error) => vec![Marked {
                mark: Mark::Failed,
                items: vec![format!("unreadable: {error}")],
            }
            .into()],
            UnreadableWorkflow::NotAWorkflow(problems) => {
                problems.iter().cloned().map(Block::from).collect()
            }
        }
    }
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
