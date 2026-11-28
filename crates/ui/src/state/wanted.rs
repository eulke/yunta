//! See [`super`]. What a stage's wants ask of the state, carried out: work under a ticket of
//! its own, work let go, a confirmation stood over the stage, a stage opened over it or in its
//! place, going back, going down to the field, leaving. A stage opened over another leaves it
//! waiting on its work; a stage left for good leaves as `routed` says, and the stage that comes
//! back into view, gone back or gone down to, asks what it asks as the person comes back to it.
//!
//! Going back from a new run keeps what was typed on it: the request goes back into the first
//! request's line where the new run was opened over it; otherwise the state keeps the new run
//! as it was left, and the next new run opened, whichever way, is that one opened again, until
//! a run starts. The stages a new run opens over hold no request of their own, so the state
//! keeps it.

use super::routed::{Asker, Standing};
use super::State;
use crate::dialog::Dialog;
use crate::stage::Want;
use crate::{Request, Stage};

impl State {
    /// What `by`'s `wants` ask for, carried out: work under a ticket of its own, work let go,
    /// what the performer holds released; and, where `by` is who the person is at, as it
    /// wanted them, a confirmation stood over the stage, a stage opened over it, going back,
    /// going down or leaving. A stage under the one the person is at moves no one.
    pub(super) fn wanted(&mut self, by: Asker, wants: Vec<Want>) -> Vec<Request> {
        let moves = by == Asker::TheApplication || by == self.on_screen();
        let mut asked = Vec::new();
        for want in wants {
            match want {
                Want::Work(operation) => asked.extend(self.asked(operation, by)),
                Want::LetGo(operation) => {
                    asked.extend(self.let_go(by, |waited| *waited == operation))
                }
                Want::Release(ticket) => asked.push(Request::LetGo(ticket)),
                _ if !moves => {}
                Want::Show(address) => asked.push(Request::ShowPage(address)),
                Want::Confirm(confirmation) => {
                    self.dialog = Some(Dialog::Confirmation(confirmation));
                }
                Want::Open(stage) => asked.extend(self.opened_over(stage)),
                Want::Replace(stage) => asked.extend(self.replaced_by(stage)),
                Want::Back => asked.extend(self.back()),
                Want::BackToItsDecision => asked.extend(self.back_to_its_decision()),
                Want::ToTheField(said) => asked.extend(self.down_to_the_field(said)),
                Want::Leave => asked.extend(self.leaving()),
            }
        }
        asked
    }

    /// `stage` opened over the one the person is at, which goes on waiting on its work under
    /// it, and what `stage` asks for as it opens: a new run, the one the state keeps left
    /// unstarted opened again as it opens, where it keeps one.
    fn opened_over(&mut self, stage: Stage) -> Vec<Request> {
        let stage = match (stage, self.unstarted.take()) {
            (Stage::Start(opening), Some(kept)) => Stage::Start(kept.opened_again(opening)),
            (stage, kept) => {
                self.unstarted = kept;
                stage
            }
        };
        self.under.extend(self.stage.take());
        self.placed(stage)
    }

    /// `stage` in the place of the one the person is at, which leaves for good, and what
    /// `stage` asks for as it opens; what that one was opened over stays under it.
    fn replaced_by(&mut self, stage: Stage) -> Vec<Request> {
        let mut asked = Vec::new();
        if let Some(left) = self.stage.take() {
            asked.extend(self.gone(&left));
        }
        asked.extend(self.placed(stage));
        asked
    }

    /// `stage` standing where the person is, as someone new, and what it asks for as it opens.
    fn placed(&mut self, stage: Stage) -> Vec<Request> {
        let standing = self.stood(stage);
        let (by, opening) = (Asker::Stage(standing.id), standing.stage.opening());
        self.stage = Some(standing);
        self.wanted(by, opening)
    }

    /// Back to the stage the one the person is at was opened over, which leaves for good, and
    /// that stage back in view. A new run left goes back as what was typed on it: its request
    /// into the first request's line where it was opened over the first request, else kept
    /// whole for the next new run. Nothing where it was opened over none.
    pub(super) fn back(&mut self) -> Vec<Request> {
        let Some(mut below) = self.under.pop() else {
            return Vec::new();
        };
        let mut asked = Vec::new();
        if let Some(left) = self.stage.take() {
            asked.extend(self.gone(&left));
            if let Stage::Start(left) = left.stage {
                match &mut below.stage {
                    Stage::FirstRun(first_run) => first_run.typed_again(left.typed_request()),
                    _ => self.unstarted = Some(left),
                }
            }
        }
        asked.extend(self.back_in_view(below));
        asked
    }

    /// Down to the nearest field under the stage the person is at, `said` noted on it; where
    /// no field stands under it, down to the stage at the bottom of what stands, the setup, the
    /// signpost or the first request, which takes no note: each stage left leaves for good, and
    /// the stage gone down to comes back into view. No stage is built and the one at the bottom
    /// is never dropped here; the first request gives way to the field once the field holds a
    /// run, as it does whenever a person comes back to it, and that field takes the note.
    /// Nothing where the stage the person is at stands at the bottom.
    fn down_to_the_field(&mut self, said: Option<String>) -> Vec<Request> {
        let field = self
            .stages_under()
            .rposition(|stage| matches!(stage, Stage::Field(_)));
        let Some(at) = field.or((!self.under.is_empty()).then_some(0)) else {
            return Vec::new();
        };
        let left = self.under.split_off(at + 1);
        let mut asked = Vec::new();
        for left in left.into_iter().chain(self.stage.take()) {
            asked.extend(self.gone(&left));
        }
        let below = self.under.pop().expect("a stage stands at `at`");
        asked.extend(self.back_in_view(below));
        if let Some(Standing {
            stage: Stage::Field(field),
            ..
        }) = self.stage.as_mut()
        {
            field.noting(said);
        }
        asked
    }

    /// `below` back in view, the person come back to it, and what it asks as they come back,
    /// which may stand another stage in its place.
    fn back_in_view(&mut self, mut below: Standing) -> Vec<Request> {
        let returned = below.stage.returned(&self.followed_runs);
        let by = Asker::Stage(below.id);
        self.stage = Some(below);
        self.wanted(by, returned)
    }

    /// Leaving the application: what the performer holds for the stage the person is at
    /// and for each under it let go, then leaving.
    fn leaving(&self) -> Vec<Request> {
        self.stage
            .iter()
            .chain(&self.under)
            .filter_map(|standing| standing.stage.held())
            .map(Request::LetGo)
            .chain([Request::Leave])
            .collect()
    }
}
