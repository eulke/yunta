//! See [`super`]. The frame a run's screen stands in: the tally and the attention row, as
//! `FollowedRuns::framing` composes them from where the person stands on the run; the
//! legend of the run's scene, the far keys beside it where no line is typed, and help over
//! it, which lists the scene's keys and every act a key takes, the legend's and those it
//! leaves to the palette, a prompt in place's keys said in its words in their stead; and the
//! note, in the attention row's place until the next key.
//!
//! On the run's screen the run's own decision stands in sight while its scene asks in
//! place (a round, the desk, a card) or its pane says why it could not be read, in its
//! pane's keeping while the run leads otherwise, and out of sight while the person leads. A
//! line is typed there while the palette stands over it, and while a round stands in place.
//!
//! The note says what the last piece of work came to where it did not do what was asked,
//! that a round put aside recorded nothing, who answered elsewhere what a prompt in place
//! asked, its prompt gone with its pause, or what reading the run remarked on. A remark is
//! said once, the first time the stage is where the person is after it came: as the state
//! folds it, or as the person comes back to the run from a stage over it. Remarks are said one at a time, and none takes the place of
//! a note standing: each waits for the key that takes the note before it away. News leave a
//! note standing, but news that take a prompt away, whose note says why in its place; the
//! next key takes it away, `/` and `?` among them, which open a dialog over the screen.

use chrono::{DateTime, Utc};
use surco::components::{Legend, Noted, Screen};
use surco::doc::Doc;
use surco::glyphs::{Glyphs, Mark};
use surco::look::Look;
use yunta_api::Remark;
use yunta_render::screen::{PromptKeys, RunKeys};

use super::{Framed, RunStage};
use crate::followed_runs::{ItsDecision, Standpoint};
use crate::FollowedRuns;

/// What the run's screen notes in its attention row's place until the next key. Not Surco's
/// `Noted`, the note as a screen holds it, nor a `Note`, a muted paragraph inside a page.
#[derive(Debug, Clone)]
pub(super) enum Notice {
    /// An answer given here is on the run's log, and what the run does now, as render says
    /// it for the option chosen or for a round's answers.
    Recorded(String),
    /// A round of questions put aside here: nothing of it is on the run's log, which still
    /// waits on it, as render says it.
    NothingRecorded,
    /// What a prompt in place asked was answered elsewhere, its prompt gone with its pause:
    /// who answered and how, in the log's words, as render says it.
    AnsweredElsewhere(String),
    /// Work the stage asked for was refused, as the refusal says.
    Refused(String),
    /// Work the stage asked for failed, as it says.
    Failed(String),
    /// What reading the run remarked on.
    Remarked(Box<Remark>),
}

impl Notice {
    /// It as a screen's note, `glyphs` drawing what a remark says: an answer recorded here or
    /// elsewhere, a round put aside and a refusal under the done mark, as the design notes
    /// what an act came to; a failure under the failed mark; a remark in render's words, under
    /// the mark its weight takes.
    fn noted(&self, glyphs: Glyphs) -> Noted {
        let (mark, said) = match self {
            Notice::Recorded(said) | Notice::AnsweredElsewhere(said) | Notice::Refused(said) => {
                (Mark::Done, said.clone())
            }
            Notice::NothingRecorded => {
                let said = yunta_render::screen::NOTHING_RECORDED.to_string();
                (Mark::Done, said)
            }
            Notice::Failed(said) => (Mark::Failed, said.clone()),
            Notice::Remarked(remark) => {
                let said = crate::remark::said(remark, glyphs);
                (said.weight.mark(), said.words)
            }
        };
        Noted { mark, said }
    }
}

impl RunStage {
    /// `screen`, of the run `framed` as `followed` keeps it at `now`, in its frame: the tally
    /// and the attention row from where the person stands on the run, a line typed there
    /// where `typed` or on the prompt in place, which marks the screen typed; the legend of its
    /// scene, with the far keys beside it where no line is typed; and the note where one
    /// stands, drawn in `look`.
    pub(super) fn in_its_frame(
        &self,
        screen: &mut Screen,
        framed: Option<&Framed<'_>>,
        followed: &FollowedRuns,
        typed: bool,
        look: &Look,
        now: DateTime<Utc>,
    ) {
        screen.typed = self.typing_in(framed);
        let typed = typed || screen.typed;
        let at = self.standpoint(framed, typed);
        (screen.tally, screen.attention) = followed.framing(at, now);
        screen.legend = self.legend(framed, followed.can_ask_for_a_run());
        screen.anywhere = match typed {
            true => Legend::new(),
            false => yunta_render::screen::anywhere(),
        };
        screen.noted = self.notice.as_ref().map(|notice| notice.noted(look.glyphs));
    }

    /// Where the person stands on the run `framed`, a line typed there where `typed`: what
    /// the attention row is read from, and where `a` goes.
    pub(super) fn standpoint(&self, framed: Option<&Framed<'_>>, typed: bool) -> Standpoint<'_> {
        Standpoint {
            run: Some((&self.run, self.its_decision(framed))),
            typed,
        }
    }

    /// Where the run `framed`'s own decision stands on this stage: in sight while its scene
    /// asks in place, or while its pane says why it could not be read where its card would
    /// stand; in its pane's keeping while the run leads otherwise; out of sight while the
    /// person leads.
    fn its_decision(&self, framed: Option<&Framed<'_>>) -> ItsDecision {
        let scene = framed.and_then(|framed| self.scene_of(framed));
        let asks_in_place = scene.is_some_and(|scene| scene.asks_in_place());
        let unread_in_place = framed.and_then(|framed| self.unread_in_its_pane(framed));
        match (asks_in_place || unread_in_place.is_some(), self.leads()) {
            (true, _) => ItsDecision::InSight,
            (false, true) => ItsDecision::InItsPane,
            (false, false) => ItsDecision::OutOfSight,
        }
    }

    /// The keys of the run `framed`'s scene in render's words: the prompt's where one stands
    /// in place; else what `j k` move, the acts of its own the legend names among them, those
    /// the scene names (`acts`), a new run where the scene ends in one and one `asks_for_a_run`
    /// here, and the field, saying
    /// whether the run keeps going; before the run is read, with no node to move along and
    /// no act to take, the field alone. The acts the palette lists stand out of it, and so do
    /// the keys that move through the pane, which a run's screen shares with every page and
    /// so does not spend its keys' row on.
    fn legend(&self, framed: Option<&Framed<'_>>, asks_for_a_run: bool) -> Legend {
        let Some(framed) = framed else {
            return yunta_render::screen::unread_run_legend();
        };
        if let Some(held) = self.in_place_in(Some(framed)) {
            return held.legend();
        }
        let scene = self.scene_of(framed);
        let offered = self.offered_in(Some(framed), asks_for_a_run);
        let acts: Vec<(char, &str)> = offered
            .iter()
            .filter_map(|act| Some((act.key?, act.named?)))
            .collect();
        let keys = RunKeys {
            along: self.j_and_k_move(Some(framed)),
            acts: &acts,
            new_run: asks_for_a_run && scene.is_some_and(|scene| scene.names_a_new_run()),
            leaving: framed.leaving(),
        };
        yunta_render::screen::run_legend(&keys, self.leads())
    }

    /// Help over the run `followed` keeps at `now`, whose screen's legend is `legend`: the
    /// keys every screen answers to and the run's own in render's words, what `j k` move
    /// along, each act its scene offers by a key of its own, in the words the legend says it
    /// where it names it and the palette's elsewhere, and where `q` leaves the run; under a
    /// prompt in place, every key it takes first in its words, those its legend names and
    /// those it leaves to help, no row of the run's for them, and none a line typed there
    /// swallows; before the run is read, the keys of its legend, `esc` beside `q`, a new run
    /// where one can be asked for, and what needs the person.
    pub(crate) fn help(&self, legend: &Legend, followed: &FollowedRuns, now: DateTime<Utc>) -> Doc {
        let Some(framed) = self.framed(followed, now) else {
            let new_run = followed.can_ask_for_a_run();
            return yunta_render::screen::help_over_an_unread_run(legend, new_run);
        };
        let offered = self.offered_in(Some(&framed), followed.can_ask_for_a_run());
        let acts: Vec<(char, &str)> = offered
            .iter()
            .filter_map(|act| Some((act.key?, act.said())))
            .collect();
        let in_place = self.in_place_in(Some(&framed));
        let in_place = in_place.map(|held| (held.keys_for_help(), held.typing()));
        let prompt = in_place.as_ref().map(|(legend, typed)| PromptKeys {
            legend,
            typed: *typed,
        });
        let along = self.j_and_k_move(Some(&framed));
        yunta_render::screen::help_over_a_run(along, &acts, prompt, framed.leaving())
    }

    /// The note it stood on gone with a key, one that opens a dialog over the stage among
    /// them, and the next remark not yet said of the run `followed` keeps in its place.
    pub(crate) fn noted_away(&mut self, followed: &FollowedRuns) {
        self.notice = None;
        self.remark_taken(followed);
    }

    /// The first remark the run in view `followed` keeps that the stage has not said, taken
    /// as its note where none stands, and said from then on.
    pub(super) fn remark_taken(&mut self, followed: &FollowedRuns) {
        if self.notice.is_some() {
            return;
        }
        let in_view = followed.in_view.as_deref();
        let in_view = in_view.filter(|in_view| *in_view.run.run() == self.run);
        let mut remarks = in_view.into_iter().flat_map(|in_view| &in_view.remarks);
        let unsaid = remarks.find(|remark| !self.remarked.contains(remark));
        let Some(unsaid) = unsaid.cloned() else {
            return;
        };
        self.remarked.push(unsaid.clone());
        self.notice = Some(Notice::Remarked(Box::new(unsaid)));
    }
}
