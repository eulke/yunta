//! See [`super`]. The workflows the repository can run, as `yunta list` lists them, a row
//! each in the order a screen lists them, with one pointed at and moved through with `j` and
//! `k`, and beside them, at the right, what `yunta list` says of the one pointed at and what
//! can be done with it, which stands under them where the screen gives it no room. Enter
//! starts a run of the one pointed at, in a new run with it chosen, as the step that runs it
//! opens one; `c` checks it as `yunta check` does, its page and what its past runs say
//! standing in the catalog's place until Escape takes the person back to it; `g` draws it and
//! `h` reads its past runs, each in a stage of its own opened over the catalog. `n` asks for
//! a new workflow as `yunta new -i` does (`new_workflow`), `t` runs the repository's cases as
//! `yunta test` does, each shown as it ends (`cases`), and `p` opens the packs installed,
//! audited, removed and written as `yunta pack` does (`packs`); `s` opens the shapes of the
//! documents Yunta reads, as `yunta schema` shows them. What it offers the palette, each act
//! with its key, and the keys help lists over it are `acts`'. A person who comes back to the catalog, from a start or a
//! workflow written, finds it read again. It stands in the
//! frame of the field it was opened over (`over_the_field`). Where nothing is typed, as on
//! every page, `a` goes where its attention row names, `n` opens a new run over it and `q`
//! goes down to the field, but where what stands takes the key for an act of its own: the
//! workflows `n` for a new workflow, the packs' listing `a` for an audit and `n` for a new
//! pack.

mod acts;
mod answered;
mod cases;
mod new_workflow;
mod packs;
mod page;
mod screen;

use chrono::{DateTime, Utc};
use yunta_api::{Remark, WorkflowCheck};
use yunta_core::step::{Arg, NextStep};
use yunta_core::WorkflowName;
use yunta_render::page::Catalog;

use super::over_the_field::attending_over_the_field;
use super::{Stage, StatsStage, Want};
use crate::dialog::Confirmation;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, News, Operation, PackOperation, WorkflowOperation};
use cases::CasesRun;
use new_workflow::NewWorkflowAsked;
use packs::PacksView;

/// The workflows the repository can run, one pointed at.
#[derive(Debug, Clone)]
pub struct CatalogStage {
    /// The workflows, once read.
    catalog: Option<Box<Catalog>>,
    /// Whether the workflows are being read: as the stage opens, as a person comes back to it
    /// and once what it changed is written, until what reading them came to lands.
    reading: bool,
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
            reading: true,
            pointed: None,
            shown: Shown::Catalog,
            scroll: Scroll::default(),
            said: None,
        }
    }

    /// What the stage asks for as it opens: the workflows.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::ReadCatalog)]
    }

    /// What the stage asks for as a person comes back to it: the workflows, read again.
    pub(crate) fn returned(&mut self) -> Vec<Want> {
        self.reading = true;
        self.opening()
    }

    /// Whether what stands takes `key` for an act of its own, where every other page takes it
    /// for what it does everywhere nothing is typed: the workflows `n` for a new workflow, and
    /// the packs' listing `a` for an audit and `n` for a new pack.
    pub(super) fn takes_itself(&self, key: char) -> bool {
        match (&self.shown, key) {
            (Shown::Catalog, 'n') => true,
            (Shown::Packs(packs), 'a' | 'n') => packs.on_the_listing(),
            _ => false,
        }
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`: on the catalog, its
    /// keys; on a page shown in its place, the keys that scroll scroll it, and the rest is
    /// what that page makes of it. Where nothing is typed, `a` goes where the attention row
    /// names, `n` opens a new run over the stage and `q` goes down to the field, but where
    /// what stands takes `a` or `n` itself.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        if self.shows_a_page() && Scroll::moves_by(stroke) {
            self.scroll.pressed(stroke);
            return Vec::new();
        }
        if !self.typing() {
            match stroke {
                Stroke::Insert('a') if !self.takes_itself('a') => {
                    return attending_over_the_field(followed, now);
                }
                Stroke::Insert('n') if !self.takes_itself('n') && followed.can_ask_for_a_run() => {
                    return Want::taking(&NextStep::Run {
                        workflow: Arg::Blank,
                    });
                }
                Stroke::Insert('q') => return vec![Want::ToTheField(None)],
                _ => {}
            }
        }
        let standing = self.standing();
        let wants = self.pressed_where_it_stands(stroke, followed.can_ask_for_a_run());
        self.rewound_unless(standing);
        wants
    }

    /// What `stroke` asks for where the catalog stands, a run asked for only where one
    /// `asks_for_a_run` here.
    fn pressed_where_it_stands(&mut self, stroke: Stroke, asks_for_a_run: bool) -> Vec<Want> {
        let next = match &mut self.shown {
            Shown::Catalog => return self.pressed_on_the_catalog(stroke, asks_for_a_run),
            Shown::Checking { name } => match stroke {
                Stroke::Decline => Next::LetGo(Operation::Workflow(WorkflowOperation::Check {
                    workflow: name.clone(),
                })),
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            Shown::Checked { .. } => match stroke {
                Stroke::Decline | Stroke::Enter => Next::Done,
                Stroke::Interrupt => Next::Leave,
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
                    true => self.returned(),
                    false => Vec::new(),
                }
            }
            Next::Leave => vec![Want::Leave],
        }
    }

    /// What `stroke` asks for on the catalog: Enter a new run of the workflow pointed at, where
    /// one `asks_for_a_run` here.
    fn pressed_on_the_catalog(&mut self, stroke: Stroke, asks_for_a_run: bool) -> Vec<Want> {
        self.said = None;
        match stroke {
            Stroke::Down | Stroke::Tab | Stroke::Insert('j') => self.moved(1),
            Stroke::Up | Stroke::BackTab | Stroke::Insert('k') => self.moved(-1),
            Stroke::Enter if asks_for_a_run => self.started(),
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
            Stroke::Interrupt => vec![Want::Leave],
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

    /// A run of the workflow pointed at, started in a new run with it chosen, as its step
    /// opens one; nothing where it does not read.
    fn started(&self) -> Vec<Want> {
        let (Some(catalog), Some(name)) = (&self.catalog, &self.pointed) else {
            return Vec::new();
        };
        let reads = catalog
            .offered
            .iter()
            .any(|offered| offered.name == *name && offered.workflow.is_ok());
        let run = NextStep::Run {
            workflow: Arg::Given(name.clone()),
        };
        match reads {
            true => Want::taking(&run),
            false => Vec::new(),
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
    /// its file gives it, which its runs are kept under, its trail naming it as the catalog
    /// offers it; nothing where it does not read.
    fn past_runs(&self) -> Vec<Want> {
        let (Some(workflow), Some(name)) = (self.pointed_runs_kept_as(), self.pointed.clone())
        else {
            return Vec::new();
        };
        let past = StatsStage::of_workflow(workflow).offered_as(name);
        vec![Want::Open(Stage::Stats(past))]
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

    /// The work its screen says is under way: the workflows read, a workflow checked, a new
    /// one written, the cases run, or the packs' work, until it ends.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        match &self.shown {
            Shown::Catalog => {
                (self.catalog.is_none() && self.reading).then_some(Operation::ReadCatalog)
            }
            Shown::Checking { name } => Some(Operation::Workflow(WorkflowOperation::Check {
                workflow: name.clone(),
            })),
            Shown::Checked { .. } => None,
            Shown::New(asked) => asked.shown_under_way(),
            Shown::Cases(cases) => cases.shown_under_way(),
            Shown::Packs(packs) => packs.shown_under_way(),
        }
    }

    /// What an act the catalog asked to be confirmed does once it is: its work, and on the
    /// packs, a pack installed from its copy shows that it is installing.
    pub(crate) fn confirmed(&mut self, operation: Operation) -> Vec<Want> {
        if let Shown::Packs(packs) = &mut self.shown {
            packs.confirmed(&operation);
        }
        vec![Want::Work(operation)]
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

    /// Whether every printable key types where the catalog stands: a new workflow's name or
    /// its shapes narrowed, a new pack's name, where a pack is or the ref it moves to.
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
}
