//! See [`super`]. A new run: one screen holding a row for each thing a run is started with, as
//! `yunta run` takes them: the request, in the box the first request asks it in; the workflow,
//! among those the catalog offers that read; each other input the workflow declares, typed; its
//! mode, among those it declares; and the adapter its sessions are pinned to, or none, for its
//! runners to say. They stand as one Surco form, each in render's words (`page::NewRun`), and
//! under them stands what the chosen workflow runs in the chosen mode and where it stops for the
//! person, which moves with those two choices and with nothing else.
//!
//! The keys act on one row at a time, the row in focus: `tab` and `shift-tab` move it from row
//! to row, round from the last to the first; on a typed row printable keys type and `←→` move
//! in the line; on the workflows `↑↓` move the pointer; on the mode and the adapter `←→` change
//! the word chosen. The screen is one a line is typed on whichever row the keys act on, so no
//! printable key opens the palette or help here, and its legend says the keys of the row in
//! focus first.
//!
//! A new run opens before anything is read for it, on the workflow a step names or none, with
//! the request typed at the first request where it was opened there (`Stage::opened_by`, every
//! way into it); as it opens it reads the catalog, and the workflow's row stands once it lands.
//! One left unstarted and opened again takes the catalog it is opened with, what its rows hold
//! kept by name (`opened_again`).
//!
//! What the rows hold is the [`RunRequest`] `yunta run` builds from the same choices: the
//! request stands in the input the workflow reads it from, and a workflow that reads none says
//! so under the box; an input left empty is not named; the mode and the adapter open on what
//! `yunta run` takes when none is named, the workflow's first mode (`Workflow::first_mode`) and
//! no adapter, which leaves each session to the adapter its runner names. Moving to another
//! workflow keeps what was typed for each input by its name and opens the mode on that
//! workflow's first. The workflows stand by how often each ran in this repository, most first,
//! then in the catalog's order, and a new run opens on the first of them.
//!
//! The adapters the row offers after none are those a run here can be pinned to, which the
//! catalog's reading carries from the config the project resolved with
//! (`Catalog::pinnable`): the ones the runners name that this binary builds, the set admission
//! refuses any other pin by, so the row offers no choice the product is known to refuse, and
//! where the runners name none it holds none but the first word. A pin that a runner of the
//! chosen workflow has no candidate on is still admission's to refuse, said in place: the row
//! does not foresee it.
//!
//! Beside the rows, at the right, stands what the chosen workflow usually takes and what this
//! machine answers for ([`WhatItTakes`](yunta_render::page::WhatItTakes), in `panel`): the
//! past runs as the catalog read them, with every workflow's history, so choosing a workflow
//! reads nothing; and what `doctor` finds at a glance, its sessions unopened, read once as the
//! new run opens and never again while it stands, the panel saying nothing of the machine until
//! it lands. Where the panel gives way, what of the machine stands in the way of the chosen
//! workflow's run stays under the rows.
//!
//! `enter` asks for that run: admitted, created and started detached, then followed in its
//! stage, which takes this one's place; while it is asked, keys wait but ctrl-c. What admission
//! refused of an input is said under that input's row until the next key, the focus moved
//! there; any other refusal or failure in the note's place, every row as it was and nothing
//! asked again; what refused or failed other work, the machine's reading or an act left going
//! under the new run, is said the same way, a start under way still waited on. `esc` goes back, keeping what was typed:
//! into the first request's line where the new run was opened there; otherwise the state keeps
//! this new run as it was left, for the next new run to open, until a run starts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use surco::components::{Noted, Region, Screen};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_api::RunRequest;
use yunta_core::step::NextStep;
use yunta_core::{AdapterId, InputName, ModeName, RunAdapter};
use yunta_render::page::doctor::Finding;
use yunta_render::page::Catalog;
use yunta_render::refusal::Refused;
use yunta_render::screen::NewRunRow;

use super::{Stage, Want};
use crate::followed_runs::Standpoint;
use crate::prompt::{Stroke, TypedLine};
use crate::{FollowedRuns, Operation, Outcome};

/// A new run, its rows as they are set.
#[derive(Debug, Clone)]
pub struct StartStage {
    /// What the person asked for, typed: what the first request held where the new run was
    /// opened there.
    request: TypedLine,
    /// The workflows the repository can run, as read when the new run opened; none until it
    /// lands.
    catalog: Option<Box<Catalog>>,
    /// The workflow chosen, by the name the catalog offers it under; until the catalog lands,
    /// the one the new run was opened on, where one was; none where the catalog offers none
    /// that reads.
    workflow: Option<String>,
    /// What is typed for each input other than the request, by its name, kept from workflow to
    /// workflow.
    inputs: BTreeMap<InputName, TypedLine>,
    /// The mode chosen, where the workflow declares modes: its first until another is.
    mode: Option<ModeName>,
    /// The adapter every session is pinned to; none leaves each to its runner.
    adapter: Option<AdapterId>,
    /// The row the keys act on.
    focus: Row,
    /// Whether the run is being admitted and started: keys wait.
    starting: bool,
    /// What refused or failed the last start, or other work it waited on, until the next key.
    said: Option<String>,
    /// What admission refused of the input in focus, said under its row until the next key.
    refusal: Option<String>,
    /// What `doctor` found of this machine as the new run opened, its sessions unopened;
    /// none until it lands.
    machine: Option<Vec<Finding>>,
}

/// A row of a new run, as the keys reach it. Not [`NewRunRow`], what its legend says of it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    Request,
    Workflow,
    Input(InputName),
    Mode,
    Adapter,
}

impl StartStage {
    /// A new run, nothing read for it yet and nothing typed: of the workflow the catalog offers
    /// as `workflow` where one is named and reads, else of the first by how often each ran
    /// here, once the catalog lands.
    pub(crate) fn opened(workflow: Option<String>) -> Self {
        StartStage {
            request: TypedLine::default(),
            catalog: None,
            workflow,
            inputs: BTreeMap::new(),
            mode: None,
            adapter: None,
            focus: Row::Request,
            starting: false,
            said: None,
            refusal: None,
            machine: None,
        }
    }

    /// A new run asked for with `request`, typed at the first request, as
    /// [`StartStage::opened`] opens one with no workflow named.
    pub(crate) fn asked(request: TypedLine) -> Self {
        StartStage {
            request,
            ..StartStage::opened(None)
        }
    }

    /// This new run, left unstarted, opened again as `opening` opens one: on the workflow
    /// `opening` names where it names one, its mode then opening on that workflow's first, as
    /// moving to another workflow does, and with its request where one was typed there; every
    /// other row as it was left, and the catalog and the machine read again as it opens, so it
    /// takes the catalog it is opened with. The focus stays on the row it was left on where the
    /// rows hold it before the catalog is read, and goes to the request otherwise.
    pub(crate) fn opened_again(self, opening: StartStage) -> StartStage {
        let typed = !opening.request.answer().value.is_empty();
        let moved = opening
            .workflow
            .as_ref()
            .is_some_and(|named| Some(named) != self.workflow.as_ref());
        let mut again = StartStage {
            request: match typed {
                true => opening.request,
                false => self.request,
            },
            mode: self.mode.filter(|_| !moved),
            workflow: opening.workflow.or(self.workflow),
            inputs: self.inputs,
            adapter: self.adapter,
            focus: self.focus,
            ..StartStage::opened(None)
        };
        if !again.rows().contains(&again.focus) {
            again.focus = Row::Request;
        }
        again
    }

    /// What the new run asks for as it opens: the workflows the repository can run, each with
    /// its past runs, and what `doctor` finds of this machine, its sessions unopened, which
    /// opens no session and spends no prompt.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![
            Want::Work(Operation::ReadCatalog),
            Want::Work(Operation::Doctor { sessions: false }),
        ]
    }

    /// What was typed as the request, as it stands: what going back to the first request puts
    /// in its line.
    pub(crate) fn typed_request(&self) -> TypedLine {
        self.request.clone()
    }

    /// The new run as a screen in `look` over what `followed` keeps at `now`: its rows, the keys
    /// on the row in focus, and what runs under them; beside them, what the chosen workflow
    /// usually takes and what this machine answers for, and what of it stands in the way of the
    /// run where that gives way; where it stands on the trail; the tally and the attention row,
    /// a line typed here; and what refused or failed the last start, in the note's place.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Screen {
        let form = self.form(look.glyphs);
        let mut screen = Screen::new(Region::new(Doc::new().with(form)));
        screen.side = Some(self.panel(look));
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            yunta_render::screen::NEW_RUN.to_string(),
        ];
        let at = Standpoint {
            run: None,
            typed: true,
        };
        (screen.tally, screen.attention) = followed.framing(at, now);
        screen.noted = self.said.clone().map(|said| Noted {
            mark: Mark::Failed,
            said,
        });
        screen.legend = yunta_render::screen::new_run_legend(self.legend_row());
        screen.typed = true;
        screen
    }

    /// What `stroke` asks for here: the row in focus typed on, moved along or changed; `tab`
    /// and `shift-tab` moving the focus; `enter` asking for the run; `esc` going back; ctrl-c
    /// leaving. While the run is asked for, keys wait but ctrl-c.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        if self.starting {
            return match stroke {
                Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            };
        }
        self.said = None;
        self.refusal = None;
        match stroke {
            Stroke::Interrupt => vec![Want::Leave],
            Stroke::Decline => vec![Want::Back],
            Stroke::Enter => self.started(),
            Stroke::Tab => {
                self.focus_moved(true);
                Vec::new()
            }
            Stroke::BackTab => {
                self.focus_moved(false);
                Vec::new()
            }
            stroke => {
                self.on_the_row(stroke);
                Vec::new()
            }
        }
    }

    /// Whether words are typed on the row in focus: the request's or an input's, while no run
    /// is being asked for.
    pub(crate) fn typing(&self) -> bool {
        !self.starting && matches!(self.focus, Row::Request | Row::Input(_))
    }

    /// What an answer to the work `asked` does: the run started is followed in its stage, which
    /// takes this one's place; the catalog read is taken, what the rows hold kept by name; what
    /// `doctor` found stands in the panel; what refused the start is said under the row it
    /// names, or in the note's place, and what failed it in the note's place, every row as it
    /// was and the keys acting again; what refused or failed any other work is said too, a
    /// start under way still waited on: a reading that refused or failed leaves the new run
    /// saying nothing of what it would have read.
    pub(crate) fn answered(&mut self, asked: &Operation, outcome: Outcome) -> Vec<Want> {
        let start = matches!(asked, Operation::Start { .. });
        match outcome {
            // A run another start asked for, left before it started, is not this one's.
            Outcome::Started { run } if self.starting => {
                let followed = Stage::opened_by(&NextStep::status(&run));
                followed.map(Want::Replace).into_iter().collect()
            }
            Outcome::Started { .. } => Vec::new(),
            Outcome::Catalog(catalog) => {
                self.read(catalog);
                Vec::new()
            }
            Outcome::Doctor(findings) => {
                self.machine = Some(findings);
                Vec::new()
            }
            Outcome::Refused(refused) if start => self.refused_start(refused),
            Outcome::Failed(said) if start => self.stood_back(said),
            Outcome::Refused(refused) => self.noting(refused.to_string()),
            Outcome::Failed(said) => self.noting(said),
            Outcome::Repository(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::PullRequest(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Detected { .. }
            | Outcome::SetupPack { .. }
            | Outcome::Initialized { .. }
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The run the rows ask for, as `yunta run` builds it from the same choices; none where no
    /// workflow is chosen.
    fn run_request(&self) -> Option<RunRequest> {
        let (name, workflow) = self.chosen()?;
        let inputs = workflow
            .inputs
            .iter()
            .filter_map(|(input, spec)| {
                let line = match spec.is_request() {
                    true => Some(&self.request),
                    false => self.inputs.get(input),
                }?;
                let value = line.answer().value;
                (!value.is_empty()).then(|| (input.clone(), value))
            })
            .collect();
        Some(RunRequest {
            workflow: PathBuf::from(name),
            inputs,
            mode: self.mode.clone(),
            adapter: self.adapter.clone().map(RunAdapter::Named),
            hermetic: false,
        })
    }

    /// The run asked for with what the rows hold; nothing where no workflow is chosen.
    fn started(&mut self) -> Vec<Want> {
        let Some(request) = self.run_request() else {
            return Vec::new();
        };
        self.starting = true;
        vec![Want::Work(Operation::Start { request })]
    }

    /// What admission `refused` of the last start, which no longer waits, said in place: under
    /// the row it names, the focus moved there, else in the note's place; the rows as they were.
    fn refused_start(&mut self, refused: Refused) -> Vec<Want> {
        let Some(row) = self.row_refused(&refused) else {
            return self.stood_back(refused.to_string());
        };
        self.starting = false;
        self.focus = row;
        self.refusal = Some(refused.to_string());
        Vec::new()
    }

    /// `said` said of the last start, which no longer waits: the rows as they were.
    fn stood_back(&mut self, said: String) -> Vec<Want> {
        self.starting = false;
        self.noting(said)
    }

    /// `said` said until the next key, of the start or of other work the new run waited on.
    fn noting(&mut self, said: String) -> Vec<Want> {
        self.said = Some(said);
        Vec::new()
    }

    /// The row in focus, as its legend says it.
    fn legend_row(&self) -> NewRunRow<'_> {
        match &self.focus {
            Row::Request => NewRunRow::Request,
            Row::Workflow => NewRunRow::Workflow,
            Row::Mode => NewRunRow::Mode,
            Row::Adapter => NewRunRow::Adapter,
            Row::Input(name) => {
                let declared = self
                    .chosen()
                    .and_then(|(_, workflow)| workflow.inputs.get_key_value(name));
                match declared {
                    Some((name, spec)) => NewRunRow::Input { name, spec },
                    None => NewRunRow::Request,
                }
            }
        }
    }
}

mod form;
mod panel;
mod rows;
