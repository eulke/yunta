//! See [`super`]. A run started: the workflow picked off the catalog, each input it
//! declares typed, the request typed at the first run standing in the input the
//! workflow reads it from, then its mode and its adapter picked, each list opening on
//! what `yunta run` takes when none is named. What is chosen is the [`RunRequest`]
//! `yunta run` builds from the same choices. The run is admitted, created and started
//! detached, what admission says standing on the screen, and then followed in its
//! stage, which takes this one's place.
//!
//! Opened from the catalog, the workflow is picked already and no request was typed:
//! the input the workflow reads its request from is asked as every other is.
//!
//! A refusal is said where the person stands, and the questions are asked again, to
//! change what was refused. Escape goes back to where the start was opened from: the
//! first run, or the catalog.

use std::collections::HashMap;
use std::path::PathBuf;

use surco::components::{Note, Prose, Region, Screen, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::{Remark, RunRequest};
use yunta_core::{AdapterId, InputName, ModeName, RunAdapter, Workflow};
use yunta_render::page::Catalog;
use yunta_render::prompt::Escape;

use super::{RunStage, Stage, Want};
use crate::prompt::{Alternative, Answerable, LineAsked, Picker, Press, Stroke};
use crate::{Operation, Outcome};

/// A run being started.
#[derive(Debug, Clone)]
pub struct StartStage {
    /// What the person asked for, typed at the first run; none where the start was
    /// opened on a workflow picked off the catalog.
    request: Option<String>,
    /// The workflows the repository can run.
    catalog: Box<Catalog>,
    /// What is chosen so far.
    chosen: Chosen,
    /// The question in front of the person.
    asking: Asking,
    /// What admission said while it admitted the run.
    remarked: Vec<Remark>,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// What a run is being started with, as far as it is chosen.
#[derive(Debug, Clone, Default)]
struct Chosen {
    /// The workflow by the name the catalog offers it under, and as it reads.
    workflow: Option<(String, Box<Workflow>)>,
    inputs: HashMap<InputName, String>,
    /// The inputs still to ask, in the order the workflow declares them.
    pending: Vec<InputName>,
    mode: Option<ModeName>,
}

/// The question in front of the person.
#[derive(Debug, Clone)]
enum Asking {
    /// Which workflow, by the name the catalog offers it under.
    Workflow(Picker<String>),
    /// One input the workflow declares.
    Input { name: InputName, line: LineAsked },
    /// Which mode it starts in.
    Mode(Picker<ModeName>),
    /// Which adapter its sessions are pinned to, or none, for its runners to say.
    Adapter(Picker<Option<AdapterId>>),
    /// Nothing: the run is being admitted and started.
    Starting,
}

impl StartStage {
    /// A run asked for with `request`, the workflow for it yet to be picked off
    /// `catalog`.
    pub(crate) fn asked(request: String, catalog: Box<Catalog>) -> Self {
        let asking = Asking::Workflow(workflows(&catalog));
        StartStage {
            request: Some(request),
            catalog,
            chosen: Chosen::default(),
            asking,
            remarked: Vec::new(),
            said: None,
        }
    }

    /// A run of the workflow `catalog` offers as `name`, picked already, with its
    /// first question in front; none where the workflow does not read.
    pub(crate) fn picked(catalog: Box<Catalog>, name: &str) -> Option<Self> {
        readable(&catalog, name)?;
        let asking = Asking::Workflow(workflows(&catalog));
        let mut start = StartStage {
            request: None,
            catalog,
            chosen: Chosen::default(),
            asking,
            remarked: Vec::new(),
            said: None,
        };
        start.picked_workflow(name.to_string());
        Some(start)
    }

    /// The questions as a screen: what was asked for, the question in front, what
    /// admission said, and what refused it.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = Doc::new();
        if let Some(request) = &self.request {
            doc = doc.with(Prose(yunta_render::screen::asked_for(request)));
        }
        doc = match &self.asking {
            Asking::Workflow(picker) => doc
                .with(Prose(yunta_render::screen::WHICH_WORKFLOW.to_string()))
                .with(picker.block(look)),
            Asking::Input { line, .. } => doc.with(line.block(look)),
            Asking::Mode(picker) => doc
                .with(Prose(yunta_render::screen::WHICH_MODE.to_string()))
                .with(picker.block(look)),
            Asking::Adapter(picker) => doc
                .with(Prose(yunta_render::screen::WHICH_ADAPTER.to_string()))
                .with(picker.block(look)),
            Asking::Starting => doc.with(Note(yunta_render::screen::STARTING.to_string())),
        };
        for remark in &self.remarked {
            doc = doc.with(Note(crate::remark::said(remark, look.glyphs).words));
        }
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen =
            Screen::new(Region::new(doc).in_sight(Sight::at_the_end_with_the_pointer()));
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            yunta_render::screen::START.to_string(),
        ];
        screen
    }

    /// What `stroke` asks for here: what the question in front makes of it, the next
    /// question once it is answered, and the run started once the last one is; Escape
    /// goes back to the first run, and Ctrl-C leaves.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match &mut self.asking {
            Asking::Workflow(picker) => match picker.press(stroke) {
                Press::Answer(name) => self.picked_workflow(name),
                other => left(other),
            },
            Asking::Input { name, line } => match line.press(stroke) {
                Press::Answer(typed) => {
                    if !typed.value.is_empty() {
                        self.chosen.inputs.insert(name.clone(), typed.value);
                    }
                    self.next_question()
                }
                other => left(other),
            },
            Asking::Mode(picker) => match picker.press(stroke) {
                Press::Answer(mode) => {
                    self.chosen.mode = Some(mode);
                    self.asking = Asking::Adapter(adapters());
                    Vec::new()
                }
                other => left(other),
            },
            Asking::Adapter(picker) => match picker.press(stroke) {
                Press::Answer(adapter) => self.starting(adapter),
                other => left(other),
            },
            Asking::Starting => match stroke {
                Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
        }
    }

    /// Whether words are being typed for an input.
    pub(crate) fn typing(&self) -> bool {
        match &self.asking {
            Asking::Input { line, .. } => line.typing(),
            Asking::Workflow(_) | Asking::Mode(_) | Asking::Adapter(_) | Asking::Starting => false,
        }
    }

    /// What admission says while it admits the run, kept to stand on the screen.
    pub(crate) fn remarked(&mut self, remark: Remark) {
        self.remarked.push(remark);
    }

    /// What an answer to the start does: the run started is followed in its stage,
    /// which takes this one's place; what refused or failed is said, and the questions
    /// are asked again.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            // A run another start asked for, left before it started, is not this one's.
            Outcome::Started { run } if matches!(self.asking, Asking::Starting) => {
                vec![Want::Replace(Stage::Run(RunStage::of(run)))]
            }
            Outcome::Started { .. } => Vec::new(),
            Outcome::Refused(refused) => self.asked_again(refused.to_string()),
            Outcome::Failed(said) => self.asked_again(said),
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
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
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Detected { .. }
            | Outcome::Initialized(_)
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The workflow named `name` chosen, the request standing in the input it reads it
    /// from, and the first of its other inputs asked.
    fn picked_workflow(&mut self, name: String) -> Vec<Want> {
        let Some(workflow) = readable(&self.catalog, &name) else {
            return Vec::new();
        };
        let mut chosen = Chosen::default();
        for (input, spec) in &workflow.inputs {
            match (spec.is_request(), &self.request) {
                (true, Some(request)) if !request.is_empty() => {
                    chosen.inputs.insert(input.clone(), request.clone());
                }
                (true, Some(_)) => {}
                (true, None) | (false, _) => chosen.pending.push(input.clone()),
            }
        }
        chosen.workflow = Some((name, Box::new(workflow)));
        self.chosen = chosen;
        self.next_question()
    }

    /// The next input to ask, or, once every one is, the mode where the workflow
    /// declares any, and its adapter.
    fn next_question(&mut self) -> Vec<Want> {
        let Some((_, workflow)) = &self.chosen.workflow else {
            return Vec::new();
        };
        if !self.chosen.pending.is_empty() {
            let name = self.chosen.pending.remove(0);
            let Some(spec) = workflow.inputs.get(&name) else {
                return self.next_question();
            };
            let line = LineAsked::new(yunta_render::screen::input_asks(&name, spec));
            self.asking = Asking::Input { name, line };
            return Vec::new();
        }
        self.asking = match modes(workflow) {
            Some(picker) => Asking::Mode(picker),
            None => Asking::Adapter(adapters()),
        };
        Vec::new()
    }

    /// The run asked for with every choice made, `adapter` last: admitted, created and
    /// started.
    fn starting(&mut self, adapter: Option<AdapterId>) -> Vec<Want> {
        let Some((name, _)) = &self.chosen.workflow else {
            return Vec::new();
        };
        let request = RunRequest {
            workflow: PathBuf::from(name),
            inputs: self.chosen.inputs.clone(),
            mode: self.chosen.mode.clone(),
            adapter: adapter.map(RunAdapter::Named),
            hermetic: false,
        };
        self.asking = Asking::Starting;
        self.remarked.clear();
        self.said = None;
        vec![Want::Work(Operation::Start { request })]
    }

    /// `said` said, and the questions asked again from the workflow.
    fn asked_again(&mut self, said: String) -> Vec<Want> {
        self.said = Some(said);
        self.chosen = Chosen::default();
        self.asking = Asking::Workflow(workflows(&self.catalog));
        Vec::new()
    }
}

/// What a question's `press` asks of the stage where it did not answer: going back to
/// where the start was opened from for one left, and leaving for Ctrl-C.
fn left<T>(press: Press<T>) -> Vec<Want> {
    match press {
        Press::Decline => vec![Want::Back],
        Press::Interrupt => vec![Want::Leave],
        Press::Answer(_) | Press::Stay | Press::Moved => Vec::new(),
    }
}

/// The workflows of `catalog` that read, by the names it offers them under.
fn workflows(catalog: &Catalog) -> Picker<String> {
    let offered = catalog
        .offered
        .iter()
        .filter(|offered| offered.workflow.is_ok())
        .map(|offered| Alternative::named(offered.name.clone(), offered.name.clone()))
        .collect();
    Picker::new(offered, Escape::GoesBack)
}

/// The workflow `catalog` offers as `name`, where it reads.
fn readable(catalog: &Catalog, name: &str) -> Option<Workflow> {
    catalog
        .offered
        .iter()
        .find(|offered| offered.name == name)
        .and_then(|offered| offered.workflow.as_ref().ok())
        .map(|readable| readable.workflow.clone())
}

/// The modes `workflow` declares, in its order, the first first, as `yunta run` starts
/// it when none is named; none where it declares none.
fn modes(workflow: &Workflow) -> Option<Picker<ModeName>> {
    let modes = workflow.modes.as_ref().filter(|modes| !modes.is_empty())?;
    let offered = modes
        .keys()
        .map(|mode| Alternative::named(mode.as_str(), mode.clone()))
        .collect();
    Some(Picker::new(offered, Escape::GoesBack))
}

/// Every adapter this binary builds, after none, which leaves it to the runners the
/// config names, as `yunta run` does when none is named.
fn adapters() -> Picker<Option<AdapterId>> {
    let named = yunta_api::machine::built_adapter_ids()
        .into_iter()
        .map(|adapter| Alternative::named(adapter.as_str().to_string(), Some(adapter)));
    let as_the_runners_say = Alternative::named(yunta_render::screen::AS_THE_RUNNERS_SAY, None);
    Picker::new(
        std::iter::once(as_the_runners_say).chain(named).collect(),
        Escape::GoesBack,
    )
}
