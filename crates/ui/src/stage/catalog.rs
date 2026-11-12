//! See [`super`]. The workflows the repository can run, as `yunta list` lists them,
//! with one pointed at and moved through with `j` and `k`. Enter starts a run of the
//! one pointed at, in the start stage with it picked; `c` checks it as `yunta check`
//! does, its page and what its past runs say standing in the catalog's place until
//! Escape takes the person back to it; `g` draws it and `h` reads its past runs, each
//! in a stage of its own opened over the catalog. `n` asks for a new workflow as `yunta new -i`
//! does (`new_workflow`), `t` runs the repository's cases as `yunta test` does,
//! each shown as it ends (`cases`), and `p` opens the packs installed, audited,
//! removed and written as `yunta pack` does (`packs`); `s` opens the shapes of the
//! documents Yunta reads, as `yunta schema` shows them. What it offers the palette, each
//! act with its key, is `acts`'. A person who comes back to
//! the catalog, from a start or a workflow written, finds it read again.

mod acts;
mod cases;
mod new_workflow;
mod packs;
mod page;

use surco::components::{Legend, Note, Region, Screen};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::{Remark, WorkflowCheck};
use yunta_core::step::{Arg, NextStep};
use yunta_core::WorkflowName;
use yunta_render::page::{Catalog, Page, PointedCatalog};

use super::{Stage, StartStage, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{News, Operation, Outcome, PackOperation, WorkflowOperation, WorkflowOutcome};
use cases::CasesRun;
use new_workflow::NewWorkflowAsked;
use packs::PacksView;

/// The workflows the repository can run, one pointed at.
#[derive(Debug, Clone)]
pub struct CatalogStage {
    /// The workflows, once read.
    catalog: Option<Box<Catalog>>,
    /// The name of the workflow the pointer stands on.
    pointed: Option<String>,
    /// What stands on the screen.
    shown: Shown,
    /// Where a page standing in the catalog's place is scrolled to.
    scroll: Scroll,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// What stands on the catalog's screen.
#[derive(Debug, Clone)]
enum Shown {
    /// The catalog, pointed at.
    Catalog,
    /// The workflow `name` being checked.
    Checking { name: String },
    /// What checking the workflow `name` came to.
    Checked {
        name: String,
        check: Box<WorkflowCheck>,
    },
    /// A new workflow, being asked for or written.
    New(Box<NewWorkflowAsked>),
    /// The cases, running or run.
    Cases(Box<CasesRun>),
    /// The packs installed, and what is done to them.
    Packs(Box<PacksView>),
}

/// What a stroke came to on a page the catalog shows in its place.
enum Next {
    /// The page stays.
    Stays,
    /// It asks for work.
    Work(Operation),
    /// It lets the work it asked for go, and stays.
    LetGo(Operation),
    /// An act, confirmed before it is done.
    Confirm(Confirmation),
    /// What the performer holds under a ticket, no longer wanted.
    Release(crate::Ticket),
    /// Back to the catalog.
    Done,
    /// The person leaves.
    Leave,
}

impl CatalogStage {
    /// The workflows, yet to be read.
    pub(crate) fn new() -> Self {
        CatalogStage {
            catalog: None,
            pointed: None,
            shown: Shown::Catalog,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens, and as a person comes back to it: the
    /// workflows, read again.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::ReadCatalog)]
    }

    /// The catalog pointed at, or the page standing in its place; what refused or
    /// failed under it; and the keys.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            yunta_render::screen::WORKFLOWS.to_string(),
        ];
        let (mut doc, legend) = match &self.shown {
            Shown::Catalog => (self.listed(look), yunta_render::screen::catalog_legend()),
            Shown::Checking { name } => {
                trail.push(name.clone());
                let checking = Note(yunta_render::screen::checking(name));
                (Doc::new().with(checking), Legend::new())
            }
            Shown::Checked { name, check } => {
                trail.push(name.clone());
                checked(check, look)
            }
            Shown::New(asked) => {
                trail.push(yunta_render::screen::NEW.to_string());
                asked.screen(look)
            }
            Shown::Cases(cases) => {
                trail.push(yunta_render::screen::CASES.to_string());
                cases.screen(look)
            }
            Shown::Packs(packs) => {
                trail.push(yunta_render::screen::PACKS.to_string());
                trail.extend(packs.trail());
                packs.screen(look)
            }
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen = Screen::new(Region::new(doc).in_sight(self.sight()));
        screen.trail = trail;
        screen.legend = self.scrolled(legend);
        screen
    }

    /// The catalog as a person moves through it, or that it is being read.
    fn listed(&self, look: &Look) -> Doc {
        let Some(catalog) = &self.catalog else {
            let reading = yunta_render::screen::READING_THE_WORKFLOWS.to_string();
            return Doc::new().with(Note(reading));
        };
        let page = PointedCatalog {
            catalog,
            pointed: self.pointed.as_deref().unwrap_or_default(),
        };
        page.document(look)
    }

    /// What `stroke` asks for here: on the catalog, its keys; on a page shown in its
    /// place, the keys that scroll scroll it, and the rest is what that page makes of it.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        if self.shows_a_page() && Scroll::moves_by(stroke) {
            self.scroll.pressed(stroke);
            return Vec::new();
        }
        let standing = self.standing();
        let wants = self.pressed_where_it_stands(stroke);
        self.rewound_unless(standing);
        wants
    }

    /// What `stroke` asks for where the catalog stands.
    fn pressed_where_it_stands(&mut self, stroke: Stroke) -> Vec<Want> {
        let next = match &mut self.shown {
            Shown::Catalog => return self.pressed_on_the_catalog(stroke),
            Shown::Checking { name } => match stroke {
                Stroke::Decline => Next::LetGo(Operation::Workflow(WorkflowOperation::Check {
                    workflow: name.clone(),
                })),
                Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            Shown::Checked { .. } => match stroke {
                Stroke::Decline | Stroke::Enter => Next::Done,
                Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            Shown::New(asked) => asked.pressed(stroke),
            Shown::Cases(cases) => cases.pressed(stroke),
            Shown::Packs(packs) => packs.pressed(stroke),
        };
        self.went(next)
    }

    /// What a page's `next` asks of the stage: work, work let go, back to the
    /// catalog, read again where a workflow was written, or leaving.
    fn went(&mut self, next: Next) -> Vec<Want> {
        match next {
            Next::Stays => Vec::new(),
            Next::Work(operation) => {
                self.said = None;
                vec![Want::Work(operation)]
            }
            Next::LetGo(operation) => {
                if matches!(self.shown, Shown::Checking { .. }) {
                    self.shown = Shown::Catalog;
                }
                vec![Want::LetGo(operation)]
            }
            Next::Confirm(confirmation) => vec![Want::Confirm(confirmation)],
            Next::Release(ticket) => vec![Want::Release(ticket)],
            Next::Done => {
                let written = match &self.shown {
                    Shown::New(asked) => asked.written().map(str::to_string),
                    _ => None,
                };
                // What packs were installed, updated or removed changes the workflows.
                let read_again = written.is_some() || matches!(self.shown, Shown::Packs(_));
                self.shown = Shown::Catalog;
                if let Some(name) = written {
                    self.pointed = Some(name);
                }
                match read_again {
                    true => vec![Want::Work(Operation::ReadCatalog)],
                    false => Vec::new(),
                }
            }
            Next::Leave => vec![Want::Leave],
        }
    }

    /// What `stroke` asks for on the catalog.
    fn pressed_on_the_catalog(&mut self, stroke: Stroke) -> Vec<Want> {
        self.said = None;
        match stroke {
            Stroke::Down | Stroke::Tab | Stroke::Insert('j') => self.moved(1),
            Stroke::Up | Stroke::BackTab | Stroke::Insert('k') => self.moved(-1),
            Stroke::Enter => self.started(),
            Stroke::Insert('c') => self.checked_asked(),
            Stroke::Insert('g') => self.drawn(),
            Stroke::Insert('h') => self.past_runs(),
            Stroke::Insert('s') => Want::taking(&NextStep::Schemas),
            Stroke::Insert('n') => {
                self.shown = Shown::New(Box::new(NewWorkflowAsked::new()));
                Vec::new()
            }
            Stroke::Insert('t') => {
                self.shown = Shown::Cases(Box::new(CasesRun::new()));
                vec![Want::Work(Operation::Workflow(WorkflowOperation::RunCases))]
            }
            Stroke::Insert('p') => {
                self.shown = Shown::Packs(Box::new(PacksView::reading()));
                vec![Want::Work(Operation::Pack(PackOperation::List))]
            }
            Stroke::Decline => vec![Want::Back],
            Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
            _ => Vec::new(),
        }
    }

    /// The pointer moved `by` rows, around at either end.
    fn moved(&mut self, by: isize) -> Vec<Want> {
        let Some(catalog) = &self.catalog else {
            return Vec::new();
        };
        let names = catalog.names();
        if names.is_empty() {
            return Vec::new();
        }
        let at = names
            .iter()
            .position(|name| Some(*name) == self.pointed.as_deref())
            .unwrap_or_default();
        let rows = names.len() as isize;
        let to = (at as isize + by).rem_euclid(rows) as usize;
        self.pointed = names.get(to).map(|name| name.to_string());
        Vec::new()
    }

    /// A run of the workflow pointed at, started in the start stage with it picked;
    /// nothing where it does not read.
    fn started(&self) -> Vec<Want> {
        let (Some(catalog), Some(name)) = (&self.catalog, &self.pointed) else {
            return Vec::new();
        };
        match StartStage::picked(catalog.clone(), name) {
            Some(start) => vec![Want::Open(Stage::Start(start))],
            None => Vec::new(),
        }
    }

    /// The graph of the workflow pointed at, as written, opened over the catalog.
    fn drawn(&self) -> Vec<Want> {
        let Some(name) = self.pointed.clone() else {
            return Vec::new();
        };
        Want::taking(&NextStep::Graph {
            workflow: Arg::Given(name),
        })
    }

    /// The past runs of the workflow pointed at, opened over the catalog, by the name
    /// its file gives it, which its runs are kept under; nothing where it does not
    /// read.
    fn past_runs(&self) -> Vec<Want> {
        match self.pointed_runs_kept_as() {
            Some(workflow) => Want::taking(&NextStep::History { workflow }),
            None => Vec::new(),
        }
    }

    /// The name the workflow pointed at keeps its runs under, the one its file gives
    /// it; none where it does not read.
    pub(super) fn pointed_runs_kept_as(&self) -> Option<WorkflowName> {
        let (catalog, name) = (self.catalog.as_ref()?, self.pointed.as_ref()?);
        let offered = catalog
            .offered
            .iter()
            .find(|offered| offered.name == *name)?;
        let readable = offered.workflow.as_ref().ok()?;
        Some(readable.workflow.name.clone())
    }

    /// The workflow pointed at, checked.
    fn checked_asked(&mut self) -> Vec<Want> {
        let Some(name) = self.pointed.clone() else {
            return Vec::new();
        };
        self.shown = Shown::Checking { name: name.clone() };
        let check = WorkflowOperation::Check { workflow: name };
        vec![Want::Work(Operation::Workflow(check))]
    }

    /// The ticket the performer holds a fetched pack's copy under, while its install
    /// is asked: what leaving the stage lets go.
    pub(crate) fn held(&self) -> Option<crate::Ticket> {
        match &self.shown {
            Shown::Packs(packs) => packs.held(),
            Shown::Catalog
            | Shown::Checking { .. }
            | Shown::Checked { .. }
            | Shown::New(_)
            | Shown::Cases(_) => None,
        }
    }

    /// Whether words are being typed: a new workflow's name.
    pub(crate) fn typing(&self) -> bool {
        match &self.shown {
            Shown::New(asked) => asked.typing(),
            Shown::Packs(packs) => packs.typing(),
            Shown::Catalog | Shown::Checking { .. } | Shown::Checked { .. } | Shown::Cases(_) => {
                false
            }
        }
    }

    /// What the cases running hand back as they end, which keep their page where it
    /// was scrolled to.
    pub(crate) fn heard(&mut self, news: News) -> Vec<Want> {
        if let Shown::Cases(cases) = &mut self.shown {
            cases.heard(news);
        }
        Vec::new()
    }

    /// What the cases' runs and a pack's say while they are made, kept under them.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        match &mut self.shown {
            Shown::Cases(cases) => cases.remarked(remark),
            Shown::Packs(packs) => packs.remarked(remark),
            Shown::Catalog | Shown::Checking { .. } | Shown::Checked { .. } | Shown::New(_) => {}
        }
    }

    /// What an answer to the stage's work does: the workflows read, the pointer
    /// kept on the one it stood on while it is still offered; a check, a workflow
    /// written or the cases' tally, on the page that waited on it; what refused or
    /// failed, said over the catalog.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        let standing = self.standing();
        let wants = self.answered_where_it_stands(outcome);
        self.rewound_unless(standing);
        wants
    }

    /// What an answer does where the catalog stands.
    fn answered_where_it_stands(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Catalog(catalog) => {
                self.read(catalog);
                Vec::new()
            }
            Outcome::Workflow(outcome) => {
                self.workflow_answered(outcome);
                Vec::new()
            }
            Outcome::Pack(outcome) => match &mut self.shown {
                Shown::Packs(packs) => {
                    let next = packs.answered(outcome);
                    self.went(next)
                }
                _ => Vec::new(),
            },
            Outcome::Refused(refused) => self.unanswered(refused.to_string()),
            Outcome::Failed(said) => self.unanswered(said),
            Outcome::Repository(_)
            | Outcome::Field { .. }
            | Outcome::Followed
            | Outcome::Page(_)
            | Outcome::NodePage(_)
            | Outcome::Questions(_)
            | Outcome::Decision(_)
            | Outcome::Stats(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Evidence(_)
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// What work on a workflow came to, on the page that waited on it: a check, a
    /// new workflow written, or the cases' tally.
    fn workflow_answered(&mut self, outcome: WorkflowOutcome) {
        match (outcome, &mut self.shown) {
            (WorkflowOutcome::Checked(check), Shown::Checking { name }) => {
                let name = name.clone();
                self.shown = Shown::Checked { name, check };
            }
            (WorkflowOutcome::Written(written), Shown::New(asked)) => asked.answered(written),
            (WorkflowOutcome::Tested(tested), Shown::Cases(cases)) => cases.answered(tested),
            (WorkflowOutcome::Checked(_), _)
            | (WorkflowOutcome::Written(_), _)
            | (WorkflowOutcome::Tested(_), _) => {}
        }
    }

    /// The workflows `catalog` offers, read: the pointer stays on the one it stood
    /// on while it is offered, and goes to the first otherwise.
    fn read(&mut self, catalog: Box<Catalog>) {
        let names = catalog.names();
        let kept = self
            .pointed
            .as_deref()
            .is_some_and(|pointed| names.contains(&pointed));
        if !kept {
            self.pointed = names.first().map(|name| name.to_string());
        }
        self.catalog = Some(catalog);
    }

    /// `said` said: over the packs where they stand, and over the catalog
    /// otherwise.
    fn unanswered(&mut self, said: String) -> Vec<Want> {
        match &mut self.shown {
            Shown::Packs(packs) => packs.unanswered(said),
            Shown::Catalog
            | Shown::Checking { .. }
            | Shown::Checked { .. }
            | Shown::New(_)
            | Shown::Cases(_) => {
                self.said = Some(said);
                self.shown = Shown::Catalog;
            }
        }
        Vec::new()
    }
}

/// What checking a workflow came to, as `yunta check` prints it: the verdict's page,
/// then what its past runs say, where they say anything.
fn checked(check: &WorkflowCheck, look: &Look) -> (Doc, Legend) {
    let doc = check.checked.document(look);
    let doc = match check.findings.is_empty() {
        true => doc,
        false => doc.with_all(check.findings.document(look).into_blocks()),
    };
    (doc, yunta_render::screen::over_the_workflows_legend())
}
