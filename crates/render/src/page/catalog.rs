//! The repo catalog, as a person reads it: every workflow the repository and
//! its installed packs offer, by the name a person runs it with — what it
//! does, the inputs it takes, how its past runs went and the command that
//! runs it, or why its file does not read — and each pack that does not
//! read. `yunta list` draws it on a terminal, and the control plane's
//! `list_workflows` tool answers with it as Markdown, so the two never
//! drift. Reading it is its caller's. The application moves through it with
//! a workflow pointed at: [`PointedCatalog`], the same sections each led by
//! the pointer's place; on its screen, the workflows a row each in the order a
//! screen lists them ([`Catalog::listed`]), and beside them the one pointed at,
//! what the section says of it and what can be done with it.

use std::path::PathBuf;

use surco::components::{
    AtAGlance, Fields, Labelled, LabelledRow, Lead, Legend, Marked, Prose, Section,
};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::look::Look;
use surco::width::{cell_width, id_column, NAME_GAP};
use yunta_core::step::{Arg, NextStep};
use yunta_core::{AdapterId, Workflow};
use yunta_replay::history::{PriorEstimation, RunSummary};

use super::Page;
use crate::command;

/// The repo catalog — the repo's own `.yunta/workflows/` plus every
/// installed pack's, a pack entry hidden behind a repo file of the same
/// `publisher/name` — as one page: `yunta list` draws it on the terminal and
/// the `list_workflows` control-plane tool answers with it as Markdown, so
/// the two never drift. Given a project's storage, each workflow carries its
/// past runs, which its prior estimation is worked out from; a broken pack is
/// named, never silently dropped.
#[derive(Debug, Clone)]
pub struct Catalog {
    /// Each installed pack that does not read, as its error says it.
    pub broken_packs: Vec<String>,
    /// Where the catalog looked for the repository's own workflows.
    pub workflows_dir: PathBuf,
    /// Where it looked for the packs installed in it.
    pub packs_dir: PathBuf,
    pub offered: Vec<OfferedWorkflow>,
    /// The adapters a run of these workflows can be pinned to, in the order the binary builds
    /// them: those the project's runners name that this binary builds, as admission reads
    /// them; none where the project did not resolve. What a new run offers on its adapter
    /// row; the page itself draws none of them.
    pub pinnable: Vec<AdapterId>,
}

/// One workflow the catalog offers, by the name a person runs it with, as
/// its file read.
#[derive(Debug, Clone)]
pub struct OfferedWorkflow {
    pub name: String,
    pub workflow: Result<ReadableWorkflow, UnreadableWorkflow>,
}

/// A workflow that reads, and its past runs here, as its history read them.
#[derive(Debug, Clone)]
pub struct ReadableWorkflow {
    pub workflow: Workflow,
    /// Each past run of the workflow here, oldest first, as the project's history read it:
    /// what how it usually went is worked out from, and how often it ran here. None where the
    /// project did not resolve, so no history was read.
    pub past: Option<Vec<RunSummary>>,
}

impl ReadableWorkflow {
    /// How its past runs usually went, where as many stand as an estimate needs.
    pub fn usually(&self) -> Option<PriorEstimation> {
        PriorEstimation::of(self.past.as_deref()?)
    }

    /// How many times it ran here: none where no history was read.
    pub fn past_runs(&self) -> usize {
        self.past.as_ref().map_or(0, Vec::len)
    }

    /// What it says it does, on the one line a row or a title gives it: what its author wrote
    /// over lines, as a YAML block writes it, stands as one. None where it says nothing.
    pub fn does(&self) -> Option<String> {
        self.workflow
            .description
            .as_deref()
            .map(surco::text::one_line)
    }
}

/// Why a workflow file the catalog offers does not read.
#[derive(Debug, Clone)]
pub enum UnreadableWorkflow {
    /// The file could not be read, as the error says.
    File(String),
    /// It reads, but not as a workflow: each problem, quoted from it.
    NotAWorkflow(Vec<surco::components::Problem>),
}

/// The catalog as a person moves through it at a screen: every workflow led
/// by the pointer's place, the one `pointed` names carrying it. Not
/// [`Catalog`] read whole, which `yunta list` prints with no pointer.
pub struct PointedCatalog<'a> {
    pub catalog: &'a Catalog,
    /// The name of the workflow the pointer stands on.
    pub pointed: &'a str,
}

impl Page for Catalog {
    /// Each broken pack, then each workflow; where there is neither, where
    /// the catalog looked. The look's glyphs draw each input's placeholder
    /// and the separators of an estimation.
    fn document(&self, look: &Look) -> Doc {
        self.drawn(look, |_| None)
    }
}

impl Page for PointedCatalog<'_> {
    /// The catalog as `yunta list` prints it, each workflow led by the
    /// pointer's place, and the one pointed at by the pointer.
    fn document(&self, look: &Look) -> Doc {
        self.catalog.drawn(look, |name| {
            Some(Lead::Pointer {
                on: name == self.pointed,
            })
        })
    }
}

impl Catalog {
    /// Each broken pack, then each workflow led by what `lead` gives its name;
    /// where there is neither, where the catalog looked.
    fn drawn(&self, look: &Look, lead: impl Fn(&str) -> Option<Lead>) -> Doc {
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
        self.offered.iter().fold(doc, |doc, offered| {
            doc.with(offered.section(lead(&offered.name), look.glyphs))
        })
    }

    /// The workflows it offers by how often each ran here, most first, then in its own
    /// order: the order a screen lists them in, the workflows and a new run's alike. Not its
    /// own order, which `yunta list` prints.
    pub fn listed(&self) -> Vec<&OfferedWorkflow> {
        let mut listed: Vec<&OfferedWorkflow> = self.offered.iter().collect();
        listed.sort_by_key(|offered| std::cmp::Reverse(offered.past_runs()));
        listed
    }

    /// The names of the workflows it offers, in the order a screen lists them
    /// ([`Catalog::listed`]): what a pointer moves through.
    pub fn names(&self) -> Vec<&str> {
        self.listed()
            .into_iter()
            .map(|offered| offered.name.as_str())
            .collect()
    }
}

impl PointedCatalog<'_> {
    /// The workflows as the application's screen lists them, under what they are, as a list of
    /// choices stands where nothing typed narrows it: a row each, in the order a screen lists
    /// them, the pointer on the one pointed at, its name strong in a column as wide as the
    /// widest and the list's gap more, what it does after it, the row cut where its region
    /// ends; each pack that does not read under them. Where none is offered, what `yunta list`
    /// says of that.
    pub fn on_screen(&self, look: &Look) -> Doc {
        let listed = self.catalog.listed();
        if listed.is_empty() {
            return self.catalog.document(look);
        }
        let column = NameColumn {
            cells: id_column(listed.iter().map(|offered| offered.name.as_str())) + NAME_GAP,
            apart: NAME_GAP,
        };
        let rows = listed.iter().map(|offered| {
            let pointed = offered.name == self.pointed;
            let named = Line::pointed(pointed, look.glyphs).push(Tone::Strong, &offered.name);
            let does = offered
                .workflow
                .as_ref()
                .ok()
                .and_then(ReadableWorkflow::does);
            column.row(named, &offered.name, does)
        });
        let broken = (!self.catalog.broken_packs.is_empty()).then(|| Marked {
            mark: Mark::Failed,
            items: self.catalog.broken_packs.clone(),
        });
        let heading = crate::screen::WORKFLOWS_YOU_CAN_RUN.to_string();
        Doc::new()
            .with(Block::Heading(heading))
            .with(Block::Rows(rows.collect()))
            .with_all(broken.map(Block::from))
    }

    /// What the panel beside the workflows says of the one pointed at, under its name: the
    /// facts its section in `yunta list` gives, the inputs it takes, how its past runs usually
    /// went and the command that runs it, or why its file does not read; then `acts`, what
    /// can be done with it here, each with its key. Nothing where none is pointed at.
    pub fn panel(&self, look: &Look, acts: Legend) -> Doc {
        let Some(offered) = self.catalog.offered.iter().find(|o| o.name == self.pointed) else {
            return Doc::new();
        };
        let facts = match &offered.workflow {
            Ok(readable) => {
                let rows = offered
                    .facts(readable, look.glyphs)
                    .into_iter()
                    .map(|fact| LabelledRow {
                        label: Line::new()
                            .plain(surco::width::INDENT)
                            .push(Tone::Muted, fact.label()),
                        value: Some(fact.into_said()),
                    });
                vec![Labelled {
                    rows: rows.collect(),
                }
                .into()]
            }
            Err(why) => why.blocks(),
        };
        let acts = AtAGlance {
            facts: Vec::new(),
            acts: Some((crate::screen::WHAT_YOU_CAN_DO.to_string(), acts.0)),
        };
        Doc::new().with_all(facts).with(acts)
    }
}

/// The column a list of workflows sets their names in, what each does after it: the first
/// request's and the workflows' screen. Not a printed catalog's section, which says what a
/// workflow does after its name and a colon.
pub(crate) struct NameColumn {
    /// The cells a name and the space after it take.
    pub(crate) cells: usize,
    /// The fewest cells between a name that passes its column and what the workflow does.
    pub(crate) apart: usize,
}

impl NameColumn {
    /// A workflow's row: `named`, its name `name` as the list leads and tones it, then what
    /// it `does` where it says ([`ReadableWorkflow::does`]), at the column's edge, or `apart`
    /// cells after a name that passes it.
    pub(crate) fn row(&self, named: Line, name: &str, does: Option<String>) -> Line {
        match does {
            Some(does) => {
                let pad = self.cells.saturating_sub(cell_width(name)).max(self.apart);
                named.plain(" ".repeat(pad)).plain(does)
            }
            None => named,
        }
    }
}

impl OfferedWorkflow {
    /// One workflow the catalog offers, its title led by `lead`: its name and
    /// what it does, the inputs it takes, how its past runs went, and the
    /// command that runs it — or why it does not read.
    fn section(&self, lead: Option<Lead>, glyphs: surco::glyphs::Glyphs) -> Section {
        let name = self.name.as_str();
        let readable = match &self.workflow {
            Ok(readable) => readable,
            Err(why) => {
                return Section {
                    lead,
                    title: Line::new().push(Tone::Strong, name),
                    blocks: why.blocks(),
                }
            }
        };
        let mut title = Line::new().push(Tone::Strong, name);
        if let Some(does) = readable.does() {
            title = title.plain(format!(": {does}"));
        }
        let mut fields = Fields::new();
        for fact in self.facts(readable, glyphs) {
            let label = fact.label();
            fields = match fact {
                Fact::Run(line) => fields.push_command(label, line),
                fact => fields.push_if(label, fact.into_said()),
            };
        }
        Section {
            lead,
            title,
            blocks: vec![fields.into()],
        }
    }

    /// How many times it ran here: none where it does not read or no history was read.
    fn past_runs(&self) -> usize {
        self.workflow
            .as_ref()
            .map_or(0, ReadableWorkflow::past_runs)
    }

    /// The facts of `readable`, as its section says them: a row per input it takes, the first
    /// one labelled; how its past runs usually went, where as many stand as an estimate needs;
    /// and the command that runs it.
    fn facts(&self, readable: &ReadableWorkflow, glyphs: surco::glyphs::Glyphs) -> Vec<Fact> {
        let mut facts = input_facts(&readable.workflow, glyphs);
        if let Some(estimation) = readable.usually() {
            let usually = crate::spend::estimation(&estimation, glyphs);
            facts.push(Fact::Usually(usually));
        }
        facts.push(Fact::Run(command::line(&NextStep::Run {
            workflow: Arg::Given(self.name.clone()),
        })));
        facts
    }
}

/// A fact a workflow's section in `yunta list` and its panel beside the workflows say of it,
/// beside its label: an input it takes, the first of them labelled; how its past runs usually
/// went; and the command that runs it, which a section draws as a command.
enum Fact {
    Input { first: bool, said: String },
    Usually(String),
    Run(String),
}

impl Fact {
    /// What it stands beside: `inputs` over the first input and nothing over the others,
    /// `usually`, and `run`.
    fn label(&self) -> &'static str {
        match self {
            Fact::Input { first: true, .. } => "inputs",
            Fact::Input { first: false, .. } => "",
            Fact::Usually(_) => "usually",
            Fact::Run(_) => "run",
        }
    }

    /// What it says.
    fn into_said(self) -> String {
        match self {
            Fact::Input { said, .. } | Fact::Usually(said) | Fact::Run(said) => said,
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

/// One fact per input the workflow takes, the first one carrying the label.
/// Each shows its value as a placeholder, which is the look's ellipsis: three
/// dots under the ASCII look.
fn input_facts(workflow: &Workflow, glyphs: surco::glyphs::Glyphs) -> Vec<Fact> {
    let mut facts = Vec::new();
    for (n, (input_name, spec)) in workflow.inputs.iter().enumerate() {
        let description = spec
            .description()
            .map(|description| format!(": {description}"))
            .unwrap_or_default();
        facts.push(Fact::Input {
            first: n == 0,
            said: format!(
                "--input {input_name}={} ({}){description}",
                glyphs.ellipsis(),
                crate::inputs::declared(spec)
            ),
        });
    }
    facts
}
