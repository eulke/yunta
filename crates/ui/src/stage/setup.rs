//! See [`super`]. A repository Yunta was never set up in, set up: what it runs and the
//! adapters this machine answers for found first, then the questions `yunta init -i`
//! asks, under the page of what was found. The project's name and its base branch
//! each keep what was detected where the line is left empty or Escape is pressed;
//! then, where an adapter answers, each proposed runner's adapter, the first standing
//! on Escape, and its model, none leaving the runner to declare later. The answers
//! become the setup `init -i` writes from the same ones ([`ProjectDraft::answered`]).
//! What was written stands, and Enter goes on to the first run.
//!
//! The performer holds the project detected until its setup is written, under the
//! ticket that detected it: the stage says which ([`SetupStage::held`]), so leaving
//! it lets the project go. What refused or failed to write is said, and the
//! repository found and asked for again.

use std::path::{Path, PathBuf};

use surco::components::{Legend, Note, Prose, Region, Screen, Sight};
use surco::doc::Doc;
use surco::look::Look;
use yunta_api::{InitializedProject, ProjectDraft, ProjectSetup, RunnerAnswer};
use yunta_core::{AdapterId, ModelName, RunnerName};
use yunta_render::page::{Initialized, Page, Setup};
use yunta_render::prompt::{keeping, model_of, runs_on, Escape, BASE_BRANCH, PROJECT_NAME};

use super::{FirstRunStage, Stage, Want};
use crate::prompt::{Alternative, Answerable, LineAsked, Picker, Press, Stroke, Typed};
use crate::{Operation, Outcome, Ticket};

/// A repository being set up.
#[derive(Debug, Clone)]
pub struct SetupStage {
    /// Where setting it up stands.
    setting: Setting,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// Where setting a repository up stands.
#[derive(Debug, Clone)]
enum Setting {
    /// What it runs being found.
    Finding,
    /// Nothing found: what refused or failed stands, and leaving is left.
    Unfound,
    /// What was found, being answered for.
    Answering(Box<Answering>),
    /// The setup answered, being written.
    Writing {
        draft: Box<ProjectDraft>,
        setup: Box<ProjectSetup>,
    },
    /// What was written into `repo`, and what was left as it was.
    Written {
        repo: PathBuf,
        initialized: Box<InitializedProject>,
    },
}

/// What was found, the answers given so far, and the question in front of the person.
#[derive(Debug, Clone)]
struct Answering {
    draft: ProjectDraft,
    /// The ticket the performer holds the project detected under.
    held: Ticket,
    /// The name typed, empty where what was detected stands.
    name: String,
    /// The base branch typed, empty where what was detected stands.
    base_branch: String,
    /// What was answered for each runner proposed so far, in the order they are.
    runners: Vec<RunnerAnswer>,
    asking: Asking,
}

/// The question in front of the person.
#[derive(Debug, Clone)]
enum Asking {
    Name(LineAsked),
    BaseBranch(LineAsked),
    /// The adapter `runner` runs on, among those this machine answers for.
    Adapter {
        runner: RunnerName,
        picker: Picker<AdapterId>,
    },
    /// The model `runner` runs on `adapter`.
    Model {
        runner: RunnerName,
        adapter: AdapterId,
        line: LineAsked,
    },
}

/// What a stroke came to among the questions.
enum Answered {
    /// The question in front stays.
    Stays,
    /// It was answered, and the next is in front.
    Next,
    /// What was typed for a model is no model name: it is said, and asked again.
    Refused(String),
    /// Every question is answered.
    Done,
    /// The person leaves.
    Leave,
}

impl SetupStage {
    /// A repository's setup, with what it runs yet to be found.
    pub(crate) fn new() -> Self {
        SetupStage {
            setting: Setting::Finding,
            said: None,
        }
    }

    /// What the stage asks for as it opens: what setting the repository up finds.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![Want::Work(Operation::DetectProject)]
    }

    /// The ticket the performer holds the project detected under, while the setup is
    /// answered: what leaving the stage lets go.
    pub(crate) fn held(&self) -> Option<Ticket> {
        match &self.setting {
            Setting::Answering(answering) => Some(answering.held),
            Setting::Finding
            | Setting::Unfound
            | Setting::Writing { .. }
            | Setting::Written { .. } => None,
        }
    }

    /// The setup as a screen: what is being found; the page of what was found, under
    /// the name and base branch that stand, with the question in front or the setup
    /// being written; or what was written. What refused or failed is said under it.
    pub(crate) fn screen(&self, look: &Look) -> Screen {
        let mut doc = match &self.setting {
            Setting::Finding => {
                Doc::new().with(Note(yunta_render::screen::FINDING_WHAT_IT_RUNS.to_string()))
            }
            Setting::Unfound => Doc::new(),
            Setting::Answering(answering) => {
                let found = found(&answering.draft, &answering.setup(), look);
                answering.asking.asked(found, look)
            }
            Setting::Writing { draft, setup } => found(draft, setup, look)
                .with(Note(yunta_render::screen::WRITING_THE_SETUP.to_string())),
            Setting::Written { repo, initialized } => written(repo, initialized, look),
        };
        if let Some(said) = &self.said {
            doc = doc.with(Note(said.clone()));
        }
        let mut screen =
            Screen::new(Region::new(doc).in_sight(Sight::at_the_end_with_the_pointer()));
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            yunta_render::screen::SETUP.to_string(),
        ];
        if let Some(legend) = self.legend() {
            screen.legend = legend;
        }
        screen
    }

    /// The keys the screen lists: leaving while nothing is found, the keys of a line
    /// asked, and going on once the setup is written. None where a list carries its own
    /// or the setup is being written.
    fn legend(&self) -> Option<Legend> {
        match &self.setting {
            Setting::Finding | Setting::Unfound => Some(yunta_render::screen::leaving_legend()),
            Setting::Answering(answering) => match answering.asking {
                Asking::Name(_) | Asking::BaseBranch(_) | Asking::Model { .. } => {
                    Some(yunta_render::screen::setup_legend())
                }
                Asking::Adapter { .. } => None,
            },
            Setting::Writing { .. } => None,
            Setting::Written { .. } => Some(yunta_render::screen::written_legend()),
        }
    }

    /// What `stroke` asks for here: what the question in front makes of it, and the
    /// setup written once the last one is answered; Enter on what was written goes on to
    /// the first run, and `q` or Ctrl-C leaves where no line is typed on.
    pub(crate) fn pressed(&mut self, stroke: Stroke) -> Vec<Want> {
        match &mut self.setting {
            Setting::Answering(answering) => match answering.pressed(stroke) {
                Answered::Stays => Vec::new(),
                Answered::Next => {
                    self.said = None;
                    Vec::new()
                }
                Answered::Refused(said) => {
                    self.said = Some(said);
                    Vec::new()
                }
                Answered::Done => self.writing(),
                Answered::Leave => vec![Want::Leave],
            },
            Setting::Written { .. } => match stroke {
                Stroke::Enter => vec![Want::Replace(Stage::FirstRun(FirstRunStage::new()))],
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
            Setting::Finding | Setting::Unfound => match stroke {
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
            Setting::Writing { .. } => match stroke {
                Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
        }
    }

    /// Whether words are being typed on a line asked.
    pub(crate) fn typing(&self) -> bool {
        match &self.setting {
            Setting::Answering(answering) => answering.asking.typing(),
            Setting::Finding
            | Setting::Unfound
            | Setting::Writing { .. }
            | Setting::Written { .. } => false,
        }
    }

    /// What an answer to the stage's work does: what was found is asked for, what was
    /// written stands; what refused or failed is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Detected { draft, held } => {
                self.setting = Setting::Answering(Box::new(Answering::new(*draft, held)));
                Vec::new()
            }
            Outcome::Initialized(initialized) => {
                if let Setting::Writing { draft, .. } = &self.setting {
                    let repo = draft.repo.clone();
                    self.setting = Setting::Written { repo, initialized };
                }
                Vec::new()
            }
            Outcome::Refused(refused) => self.unwritten(refused.to_string()),
            Outcome::Failed(said) => self.unwritten(said),
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
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// The setup answered, asked to be written with the project held under its ticket.
    fn writing(&mut self) -> Vec<Want> {
        let Setting::Answering(answering) = &self.setting else {
            return Vec::new();
        };
        let (held, setup) = (answering.held, answering.setup());
        let draft = Box::new(answering.draft.clone());
        self.setting = Setting::Writing {
            draft,
            setup: Box::new(setup.clone()),
        };
        self.said = None;
        vec![Want::Work(Operation::WriteSetup { held, setup })]
    }

    /// `said` said: of a setup being written, which the project was taken to write, the
    /// repository found again to be asked for again; of what finding it came to, with
    /// nothing found.
    fn unwritten(&mut self, said: String) -> Vec<Want> {
        self.said = Some(said);
        match self.setting {
            Setting::Writing { .. } => {
                self.setting = Setting::Finding;
                vec![Want::Work(Operation::DetectProject)]
            }
            Setting::Finding => {
                self.setting = Setting::Unfound;
                Vec::new()
            }
            Setting::Unfound | Setting::Answering(_) | Setting::Written { .. } => Vec::new(),
        }
    }
}

impl Answering {
    /// What `draft` found, held under `held`, with the project's name asked first.
    fn new(draft: ProjectDraft, held: Ticket) -> Self {
        let asking = Asking::Name(LineAsked::new(keeping(PROJECT_NAME, &draft.name)));
        Answering {
            draft,
            held,
            name: String::new(),
            base_branch: String::new(),
            runners: Vec::new(),
            asking,
        }
    }

    /// The setup the answers given so far make.
    fn setup(&self) -> ProjectSetup {
        self.draft
            .answered(&self.name, &self.base_branch, self.runners.clone())
    }

    /// What `stroke` does to the question in front: the answer kept and the next
    /// question asked, a model name refused, or what the stage does.
    fn pressed(&mut self, stroke: Stroke) -> Answered {
        match &mut self.asking {
            Asking::Name(line) => match typed(line.press(stroke)) {
                Ok(name) => {
                    self.name = name;
                    let asked = keeping(BASE_BRANCH, &self.draft.base_branch);
                    self.asking = Asking::BaseBranch(LineAsked::new(asked));
                    Answered::Next
                }
                Err(answered) => answered,
            },
            Asking::BaseBranch(line) => match typed(line.press(stroke)) {
                Ok(base_branch) => {
                    self.base_branch = base_branch;
                    self.next_runner()
                }
                Err(answered) => answered,
            },
            Asking::Adapter { runner, picker } => {
                let adapter = match picker.press(stroke) {
                    Press::Answer(adapter) => Some(adapter),
                    Press::Decline => picker.first(),
                    Press::Interrupt => return Answered::Leave,
                    Press::Stay | Press::Moved => return Answered::Stays,
                };
                let runner = runner.clone();
                adapter.map_or(Answered::Done, |adapter| self.model_asked(runner, adapter))
            }
            Asking::Model {
                runner,
                adapter,
                line,
            } => match typed(line.press(stroke)) {
                Ok(typed) => {
                    let (runner, adapter) = (runner.clone(), adapter.clone());
                    self.modelled(runner, adapter, &typed)
                }
                Err(answered) => answered,
            },
        }
    }

    /// The model `runner` runs on `adapter` asked.
    fn model_asked(&mut self, runner: RunnerName, adapter: AdapterId) -> Answered {
        let line = LineAsked::new(model_of(&runner));
        self.asking = Asking::Model {
            runner,
            adapter,
            line,
        };
        Answered::Next
    }

    /// `runner` answered on `adapter` with the model `typed`, none where it is empty,
    /// and the next runner asked; asked again where what was typed is no model name.
    fn modelled(&mut self, runner: RunnerName, adapter: AdapterId, typed: &str) -> Answered {
        let model = match typed.is_empty() {
            true => None,
            false => match typed.parse::<ModelName>() {
                Ok(model) => Some(model),
                Err(refused) => {
                    self.model_asked(runner, adapter);
                    return Answered::Refused(refused.to_string());
                }
            },
        };
        self.runners.push(RunnerAnswer {
            runner,
            adapter,
            model,
        });
        self.next_runner()
    }

    /// The adapter of the next runner proposed asked, among those this machine answers
    /// for; every question answered where none is left or no adapter answers.
    fn next_runner(&mut self) -> Answered {
        let answering = self.draft.answering();
        let next = self.draft.proposed.roles.get(self.runners.len());
        let (Some(runner), false) = (next, answering.is_empty()) else {
            return Answered::Done;
        };
        let offered = answering
            .into_iter()
            .map(|adapter| Alternative::named(adapter.as_str(), adapter.clone()))
            .collect();
        self.asking = Asking::Adapter {
            runner: runner.clone(),
            picker: Picker::new(offered, Escape::TakesTheFirst),
        };
        Answered::Next
    }
}

impl Asking {
    /// `doc` with the question under it: the line typed on, or what the adapters are
    /// offered for over their list.
    fn asked(&self, doc: Doc, look: &Look) -> Doc {
        match self {
            Asking::Name(line) | Asking::BaseBranch(line) | Asking::Model { line, .. } => {
                doc.with(line.block(look))
            }
            Asking::Adapter { runner, picker } => {
                doc.with(Prose(runs_on(runner))).with(picker.block(look))
            }
        }
    }

    /// Whether the question is a line typed on.
    fn typing(&self) -> bool {
        match self {
            Asking::Name(line) | Asking::BaseBranch(line) | Asking::Model { line, .. } => {
                line.typing()
            }
            Asking::Adapter { picker, .. } => picker.typing(),
        }
    }
}

/// What a line's `press` answers with: the words typed, or none on Escape, which keeps
/// what was detected; what the stage does where it answered nothing.
fn typed(press: Press<Typed>) -> Result<String, Answered> {
    match press {
        Press::Answer(typed) => Ok(typed.value),
        Press::Decline => Ok(String::new()),
        Press::Interrupt => Err(Answered::Leave),
        Press::Stay | Press::Moved => Err(Answered::Stays),
    }
}

/// The page of what `draft` found, under the name and base branch `setup` stands on.
fn found(draft: &ProjectDraft, setup: &ProjectSetup, look: &Look) -> Doc {
    let writes = draft.writes();
    Setup {
        repo: &draft.repo,
        name: &setup.name,
        base_branch: &setup.base_branch,
        detected: &draft.detected,
        probed: &draft.probed,
        proposed: &draft.proposed.roles,
        writes: &writes,
    }
    .document(look)
}

/// The page of what was written into `repo`, as `init` ends with it.
fn written(repo: &Path, initialized: &InitializedProject, look: &Look) -> Doc {
    Initialized {
        repo,
        wrote: &initialized.wrote,
        skipped: &initialized.skipped,
        detected: &initialized.detected,
        probed: &initialized.probed,
        declared: &initialized.declared,
        undeclared: &initialized.undeclared,
    }
    .document(look)
}
