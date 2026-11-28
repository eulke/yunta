//! See [`super`]. A setup's rows as a person reads and changes them: the row pointed at, `j k`
//! stepping along the rows that change, and the one being changed in place, a line typed or a
//! role's adapter chosen then its model typed; the answers they come to, which the setup is
//! written from.

use std::sync::Arc;

use surco::components::Legend;
use yunta_api::{ProjectSetup, RunnerAnswer, RunnerDeclaration};
use yunta_core::{AdapterId, ModelName, RunnerName};
use yunta_render::prompt::{BASE_BRANCH, MODEL, PROJECT_NAME};
use yunta_render::screen::Kept;

use super::Found;
use crate::prompt::{Edit, Stroke, TypedLine};
use crate::SetupPackFetch;

/// What was found, the answers given so far, the row pointed at and the row being changed.
#[derive(Debug, Clone)]
pub(super) struct Rows {
    pub(super) found: Found,
    /// The name taken, empty where what was detected stands.
    name: String,
    /// The base branch taken, empty where what was detected stands.
    base_branch: String,
    /// What was taken for each role changed, by role.
    runners: Vec<RunnerAnswer>,
    pub(super) pointed: SetupRow,
    /// The row pointed at, being changed in place, where it is.
    pub(super) changing: Option<Changing>,
}

/// A row of a setup the pointer stands on: a fact that changes, a role, or the row that writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SetupRow {
    Name,
    Base,
    Role(RunnerName),
    Write,
}

/// A row being changed in place: the one pointed at.
#[derive(Debug, Clone)]
pub(super) enum Changing {
    /// The name or the base, the line typed under it.
    Typed(TypedLine),
    /// A role, its adapter chosen among those this machine answers for, by where it stands
    /// among them, and its model, the line typed under it, which the keys type in once the
    /// adapter is taken.
    Role {
        chosen: usize,
        model: TypedLine,
        on_the_model: bool,
    },
}

/// What a stroke came to on the rows.
pub(super) enum Pressed {
    /// The rows stay as they are, or moved, or a row was changed: nothing is asked.
    Stays,
    /// A model typed that is no model's name: it is said, and the line stays.
    Refused(String),
    /// The row that writes, taken.
    Write,
    /// To the run the attention row names, in the setup's place.
    Attend,
    /// The person leaves, nothing written.
    Leave,
}

impl Rows {
    /// What was `found`, as detected, the pointer on the row that writes.
    pub(super) fn new(found: Found) -> Self {
        Rows {
            found,
            name: String::new(),
            base_branch: String::new(),
            runners: Vec::new(),
            pointed: SetupRow::Write,
            changing: None,
        }
    }

    /// The pack `fetched` landed: its row and the roles it proposes stand; a role the
    /// pointer stood on that the project no longer proposes gives the pointer back to the row
    /// that writes, the role closed.
    pub(super) fn fetched(&mut self, fetched: Arc<SetupPackFetch>) {
        self.found.pack = Some(fetched);
        if !self.changing_rows().contains(&self.pointed) {
            self.pointed = SetupRow::Write;
            self.changing = None;
        }
    }

    /// The setup the rows make, of the project as the setup proposes it.
    pub(super) fn setup(&self) -> ProjectSetup {
        self.found
            .proposing()
            .answered(&self.name, &self.base_branch, self.runners.clone())
    }

    /// Whether the rows make the setup `init` writes asking nothing, of the project as the
    /// setup proposes it: none changed, or each changed back to what it proposes.
    pub(super) fn as_proposed(&self) -> bool {
        self.setup() == self.found.proposing().answered("", "", Vec::new())
    }

    /// The setup the rows make of the project as detected, proposing none of the roles of the
    /// pack fetched with it: what a setup written without that pack writes, as `init` writes it
    /// with the same answers, an answer to a role only the pack proposes left out.
    pub(super) fn setup_without_the_pack(&self) -> ProjectSetup {
        self.found
            .draft
            .answered(&self.name, &self.base_branch, self.runners.clone())
    }

    /// The rows that change, in the order `j k` step along them: the name, the base, each
    /// role proposed, the row that writes.
    fn changing_rows(&self) -> Vec<SetupRow> {
        let roles = self.found.proposing().proposed.roles().to_vec();
        [SetupRow::Name, SetupRow::Base]
            .into_iter()
            .chain(roles.into_iter().map(SetupRow::Role))
            .chain([SetupRow::Write])
            .collect()
    }

    /// The keys of the row pointed at: of the row that writes; of a row that changes; of the
    /// adapter being chosen; of the line being typed.
    pub(super) fn legend(&self) -> Legend {
        let kept = self.kept();
        match (&self.changing, &self.pointed) {
            (None, SetupRow::Write) => yunta_render::screen::setup_legend(),
            (None, _) => yunta_render::screen::setup_row_legend(),
            (
                Some(Changing::Role {
                    on_the_model: false,
                    ..
                }),
                _,
            ) => yunta_render::screen::choosing_legend(kept),
            (Some(Changing::Role { .. }), _) => yunta_render::screen::typing_legend(MODEL, kept),
            (Some(Changing::Typed(_)), SetupRow::Base) => {
                yunta_render::screen::typing_legend(BASE_BRANCH, kept)
            }
            (Some(Changing::Typed(_)), _) => {
                yunta_render::screen::typing_legend(PROJECT_NAME, kept)
            }
        }
    }

    /// How the row pointed at stands, which Escape gives back: as detected, or as changed.
    fn kept(&self) -> Kept {
        let changed = match &self.pointed {
            SetupRow::Name => !self.name.is_empty(),
            SetupRow::Base => !self.base_branch.is_empty(),
            SetupRow::Role(runner) => self.runners.iter().any(|answer| &answer.runner == runner),
            SetupRow::Write => false,
        };
        match changed {
            true => Kept::Changed,
            false => Kept::Detected,
        }
    }

    /// What `stroke` does to the rows: steps along them, opens the row pointed at, changes
    /// it, takes it or gives it back; on the row that writes, writes.
    pub(super) fn pressed(&mut self, stroke: Stroke) -> Pressed {
        if stroke == Stroke::Interrupt {
            return Pressed::Leave;
        }
        let Some(changing) = self.changing.take() else {
            return self.pressed_on_a_row(stroke);
        };
        match changing {
            Changing::Typed(line) => self.typed(line, stroke),
            Changing::Role {
                chosen,
                model,
                on_the_model: false,
            } => self.choosing(chosen, model, stroke),
            Changing::Role { chosen, model, .. } => self.modelled(chosen, model, stroke),
        }
    }

    /// `stroke` on the row pointed at, none opened: `j k` and the arrows step, `enter` opens
    /// it or writes, `a` goes where the attention row points, `q` leaves.
    fn pressed_on_a_row(&mut self, stroke: Stroke) -> Pressed {
        let rows = self.changing_rows();
        let at = rows
            .iter()
            .position(|row| *row == self.pointed)
            .unwrap_or(0);
        match stroke {
            Stroke::Insert('j') | Stroke::Down => {
                self.pointed = rows[(at + 1).min(rows.len() - 1)].clone();
                Pressed::Stays
            }
            Stroke::Insert('k') | Stroke::Up => {
                self.pointed = rows[at.saturating_sub(1)].clone();
                Pressed::Stays
            }
            Stroke::Enter => self.opening(),
            Stroke::Insert('a') => Pressed::Attend,
            Stroke::Insert('q') => Pressed::Leave,
            _ => Pressed::Stays,
        }
    }

    /// `enter` on the row pointed at: the row that writes writes; the name or the base opens
    /// a line typed under it; a role opens on its adapter among those this machine answers
    /// for, where one does.
    fn opening(&mut self) -> Pressed {
        match self.pointed.clone() {
            SetupRow::Write => return Pressed::Write,
            SetupRow::Name | SetupRow::Base => {
                self.changing = Some(Changing::Typed(TypedLine::default()));
            }
            SetupRow::Role(runner) => {
                let draft = self.found.proposing();
                let answering = draft.answering();
                let (adapter, model) = self.first_candidate(&runner).unzip();
                let chosen = adapter
                    .and_then(|adapter| answering.iter().position(|offered| **offered == adapter));
                let model = model.flatten();
                if let (Some(chosen), false) = (chosen, answering.is_empty()) {
                    self.changing = Some(Changing::Role {
                        chosen,
                        model: TypedLine::holding(model.as_ref().map_or("", ModelName::as_str)),
                        on_the_model: false,
                    });
                }
            }
        }
        Pressed::Stays
    }

    /// The adapter `runner` stands on first, and its model, as the setup writes it.
    fn first_candidate(&self, runner: &RunnerName) -> Option<(AdapterId, Option<ModelName>)> {
        let RunnerDeclaration::Declared(declared) = self.setup().runners().clone() else {
            return None;
        };
        let role = declared.into_iter().find(|role| &role.runner == runner)?;
        let first = role.candidates.into_iter().next()?;
        Some((first.adapter, first.model))
    }

    /// `stroke` on the name or the base typed in place: the line takes it; Enter takes what is
    /// typed, an empty line keeping what stands; Escape gives the row back.
    fn typed(&mut self, mut line: TypedLine, stroke: Stroke) -> Pressed {
        match line.apply(stroke) {
            Edit::Answered(typed) => {
                let taken = typed.value.trim().to_string();
                if !taken.is_empty() {
                    match self.pointed {
                        SetupRow::Base => self.base_branch = taken,
                        _ => self.name = taken,
                    }
                }
            }
            Edit::Declined => {}
            Edit::Interrupted => return Pressed::Leave,
            Edit::Editing => self.changing = Some(Changing::Typed(line)),
        }
        Pressed::Stays
    }

    /// `stroke` on a role's adapter being chosen: the arrows choose among those this machine
    /// answers for, Enter takes the one chosen and goes on to its model, Escape gives the row
    /// back.
    fn choosing(&mut self, chosen: usize, model: TypedLine, stroke: Stroke) -> Pressed {
        let offered = self.found.draft.answering().len();
        let chosen = match stroke {
            Stroke::Left => chosen.saturating_sub(1),
            Stroke::Right => (chosen + 1).min(offered.saturating_sub(1)),
            Stroke::Decline => return Pressed::Stays,
            _ => chosen,
        };
        self.changing = Some(Changing::Role {
            chosen,
            model,
            on_the_model: stroke == Stroke::Enter,
        });
        Pressed::Stays
    }

    /// `stroke` on a role's model typed in place: the line takes it; Enter takes the adapter
    /// chosen with the model typed, none where the line is empty, and is refused where what
    /// was typed names no model; Escape gives the row back.
    fn modelled(&mut self, chosen: usize, mut model: TypedLine, stroke: Stroke) -> Pressed {
        let SetupRow::Role(runner) = self.pointed.clone() else {
            return Pressed::Stays;
        };
        let reopened = |model: TypedLine| Changing::Role {
            chosen,
            model,
            on_the_model: true,
        };
        let typed = match model.apply(stroke) {
            Edit::Answered(typed) => typed.value.trim().to_string(),
            Edit::Declined => return Pressed::Stays,
            Edit::Interrupted => return Pressed::Leave,
            Edit::Editing => {
                self.changing = Some(reopened(model));
                return Pressed::Stays;
            }
        };
        let model_name = match typed.is_empty() {
            true => None,
            false => match typed.parse::<ModelName>() {
                Ok(model_name) => Some(model_name),
                Err(refused) => {
                    self.changing = Some(reopened(TypedLine::holding(&typed)));
                    return Pressed::Refused(refused.to_string());
                }
            },
        };
        let draft = self.found.draft.clone();
        let Some(adapter) = draft
            .answering()
            .get(chosen)
            .map(|adapter| (*adapter).clone())
        else {
            return Pressed::Stays;
        };
        self.runners.retain(|answer| answer.runner != runner);
        self.runners.push(RunnerAnswer {
            runner,
            adapter,
            model: model_name,
        });
        Pressed::Stays
    }
}
