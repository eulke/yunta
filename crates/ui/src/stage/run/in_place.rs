//! See [`super`]. The prompt a run's screen holds where the run's own decision stands in
//! sight, built once for the pause it belongs to, as the follower read that pause, and kept
//! while the run stands at it: news of the same pause never build it again, so what a person
//! picked or typed stays. It stands where the scene composes it (`pane`): a card at the
//! pane's foot, under what its decision rests on; a round of questions under the request it
//! is about, in the place of the nodes and the pane; the plan's desk, which heads itself,
//! between the rule and the attention row. Where the plan a decision is on stands, in front
//! of its card or behind it, is the prompt's, and the scene follows it; before the first
//! prompt is built, the stage says where it opens, on the card or on the plan, and the first
//! prompt held takes that opening, so a later pause stands as its scene says.
//!
//! `esc`, where the prompt does not take it, puts the prompt aside with nothing recorded: the
//! person takes the pointer on the node the run waits at, whose own scene says what it waits
//! on and its feed, so they can read it and the other nodes before deciding. The prompt
//! stays held for its pause, and `a` or `f`, which give the lead back to the run, bring it
//! back as the person left it. `esc` again leaves the run as it does with no prompt.
//!
//! The run standing at its pause no longer, the prompt goes, and what was typed or picked on
//! it with it, even while a person types on it: what no longer waits cannot be answered.
//! Where the log holds an answer to that pause given elsewhere, the note says who gave it, in
//! the log's words (`gone`). A prompt held and out of sight types nothing and takes no key.
//!
//! A card on a scope asked for offers granting and denying it alone: pausing, which leaves
//! the decision open, is `esc`'s there.
//!
//! A card's keys are the card's, the one prompt model a terminal's card is too
//! ([`DecisionCard`](crate::prompt::DecisionCard)): `j k`, `tab` and a number choose, `enter`
//! answers, `n` adds a note and `r` reads the plan it offers, the plan's desk standing in its
//! place, on the plan's first task, until `esc` there brings the card back as it was. While
//! words are typed, every key is the card's. Otherwise the arrows, the page keys and the wheel
//! are the stage's, for what stands above the card, which they scroll without taking anything
//! from the run: it opens at its end, nearest the card, and the card's choice never moves by
//! them. `esc` and Ctrl-C are the stage's too, and so is every key the card has no use for.
//!
//! The desk is the decision with the plan in front, the one a terminal's desk is too
//! ([`PlanDesk`]), opened on the plan's first task: a number names an
//! option and `enter` confirms it, `esc` taking the number back; `j k`, `tab` and `shift-tab`
//! go from place to place, `p` turns a place over to its proof and `d` to what was decided;
//! the arrows, the page keys and the wheel move what is read in its pane and never the place
//! or the choice. Where nothing waits to be confirmed, `esc` goes back to the card the plan
//! was read from, or else is the stage's, which puts the desk aside: a desk the plan leads
//! to, or one the stage opened on. An option that asks for words, confirmed, opens its line
//! at the desk's foot in place of the options, the desk standing: every key is the desk's
//! while words are typed there, `enter` sends them with the option, an empty line refused
//! under it, and `esc` goes back to the options. On a card, an option that asks for words, or
//! a note, opens its line under every option.
//!
//! A round is `prompt`'s [`Round`], the one a terminal's round is too: a line is typed there
//! while it stands, every key but `esc` its own, the arrows choosing in a list and every
//! printable key typing, a list narrowed by what is typed; `enter` answers and `shift-tab`
//! goes back to the question before. Nothing of it is recorded until its last answer, which
//! records every answer at once. `esc` puts it aside, the note saying nothing was recorded. A
//! round whose answers the engine refused stands with each refusal under its question.
//!
//! `enter` on an option, or on a round's last question, asks the performer to record it on
//! the run's log, once: the prompt answers no key while the answer is under way, and an
//! answer refused or failed hands it back, an option answered at the desk keeping the desk in
//! front all the while. Once the log holds it, what the run does now is noted, in render's
//! sentence for the option chosen or for the answers recorded (`answer`).

mod answer;
mod gone;

use chrono::{DateTime, Utc};
use surco::components::Legend;
use surco::doc::Block;
use surco::look::Look;
use yunta_api::{AnswerablePause, WaitingDecision, WaitingQuestions};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NodeId, RunId};
use yunta_render::prompt::card::{self, ChosenWith};
use yunta_render::prompt::{Escape, PromptAct};

use super::frame::Notice;
use super::lead::PointedBy;
use super::scene::{ComposedPane, Grounds};
use super::{Framed, RunStage};
use crate::prompt::{Answerable, Deciding, DecisionPut, Opens, PlanDesk, Round, Stroke};
use crate::scroll::Scroll;
use crate::stage::Want;
use crate::{AwaitedReading, FollowedRuns};
use answer::Answer;

/// The prompt a run's screen holds for the pause it belongs to, as `awaited` names it: the
/// decision the run waits on, answered on its card or at the plan's desk, with what stands
/// above the card as the person scrolled it, or the round of questions it waits on; and an
/// answer under way. Not a dialog, which stands over any stage.
#[derive(Debug, Clone)]
pub(super) struct InPlace {
    /// The pause it belongs to.
    pause: AnswerablePause,
    /// What it puts to the person.
    posed: Posed,
    /// Where what stands above a card is scrolled to: its end, nearest the card, until a
    /// person scrolls it back. A desk scrolls its own pane.
    above: Scroll,
    /// Where the answer given on it stands.
    answer: Answer,
}

/// What a prompt in place puts to the person: the decision the run waits on, its card or the
/// plan's desk in front, or the round of questions a node asks, as far as it is answered. Not
/// the run's [`Scene`](super::Scene), which says where it stands on the screen.
#[derive(Debug, Clone)]
enum Posed {
    Decision(Box<Deciding>),
    Round { node: NodeId, round: Box<Round> },
}

/// How a scene places a prompt in its pane: as a card resting on what its decision rests
/// on, as a round of questions, or as the desk of the plan the decision is on. Not
/// [`ComposedPane`], every pane a scene composes.
#[derive(Debug, Clone, Copy)]
enum PlacedAs {
    Card(Grounds),
    Round,
    Desk,
}

impl InPlace {
    /// The prompt of `decision`, which the run `run` waits on at `pause`, resting on
    /// `grounds`: its card in front, the pointer on its first option, `esc` putting it aside
    /// waiting, what stands above it read from its end, and the plan it offers behind it, on
    /// the plan's first task. A card on a scope asked for offers granting and denying it:
    /// pausing, which leaves it open, is `esc`'s.
    fn of(
        run: &RunId,
        decision: &WaitingDecision,
        (pause, grounds): (&AnswerablePause, Grounds),
    ) -> InPlace {
        let put = DecisionPut::waiting(run, decision);
        let deciding = Deciding::put(&put, Opens::TheCard, Escape::LeavesWaiting);
        let deciding = deciding.on_its_plans_first_task();
        let deciding = match grounds {
            Grounds::Scope => deciding.leaving_out(&ReservedOption::Abort.id()),
            Grounds::Shown | Grounds::Failure | Grounds::Departure | Grounds::Cap => deciding,
        };
        InPlace::posing(pause, Posed::Decision(Box::new(deciding)))
    }

    /// The prompt of `decision`, which the run `run` waits on at `pause`, read at the desk of
    /// the plan it is on: the plan in front, on its first task, nothing waiting to be
    /// confirmed.
    fn desk(run: &RunId, decision: &WaitingDecision, pause: &AnswerablePause) -> InPlace {
        let put = DecisionPut::waiting(run, decision);
        let deciding = Deciding::put(&put, Opens::ThePlan, Escape::LeavesWaiting);
        let deciding = deciding.on_its_plans_first_task();
        InPlace::posing(pause, Posed::Decision(Box::new(deciding)))
    }

    /// The prompt of the round `asked`, which its run waits on at `pause`: its first
    /// question in turn and nothing answered, what the engine refused of the answers it was
    /// last given standing under the questions it is about.
    fn round(asked: &WaitingQuestions, pause: &AnswerablePause) -> Self {
        let round = Round::new(&asked.node, &asked.questions, Escape::LeavesWaiting)
            .refused_as(asked.refused.as_ref());
        let posed = Posed::Round {
            node: asked.node.clone(),
            round: Box::new(round),
        };
        InPlace::posing(pause, posed)
    }

    /// The prompt that puts `posed` to the person at `pause`, no answer given yet.
    fn posing(pause: &AnswerablePause, posed: Posed) -> InPlace {
        InPlace {
            pause: pause.clone(),
            posed,
            above: Scroll::at_the_end(),
            answer: Answer::Open,
        }
    }

    /// Whether a line is typed on it, so every key types: on a round, always; on a card or
    /// the plan's desk, while words are typed for an option.
    pub(super) fn typing(&self) -> bool {
        match &self.posed {
            Posed::Decision(deciding) => deciding.typing(),
            Posed::Round { .. } => true,
        }
    }

    /// Whether `stroke` is its own: on a round, every stroke but `esc`, which is the
    /// stage's; on the plan's desk, each key the desk reads by, `enter` and `esc` while a
    /// number waits to be confirmed, and every stroke while words are typed at its foot; on a
    /// card, every stroke while words are typed, else each
    /// key the card answers to but `esc` and Ctrl-C, which are the stage's, and the keys that
    /// scroll, which move what stands above the card.
    fn takes(&self, stroke: Stroke) -> bool {
        match &self.posed {
            Posed::Round { .. } => !matches!(stroke, Stroke::Decline),
            Posed::Decision(deciding) => match deciding.desk_in_front() {
                Some(desk) => desk.takes(stroke),
                None if deciding.typing() => true,
                None => {
                    let the_stages = Scroll::moves_by(stroke)
                        || matches!(stroke, Stroke::Decline | Stroke::Interrupt);
                    !the_stages && deciding.card().takes(stroke)
                }
            },
        }
    }

    /// Whether the plan its decision is on stands in front of the card: read from the card,
    /// or opened on. Never on a round.
    fn plan_in_front(&self) -> bool {
        match &self.posed {
            Posed::Decision(deciding) => deciding.plan_in_front(),
            Posed::Round { .. } => false,
        }
    }

    /// What the note says once `esc` puts it aside: that nothing of a round was recorded;
    /// nothing of a decision, whose card or desk stands as it was left once the run leads
    /// again.
    fn noted_aside(&self) -> Option<Notice> {
        match &self.posed {
            Posed::Round { .. } => Some(Notice::NothingRecorded),
            Posed::Decision(_) => None,
        }
    }

    /// The prompt as it stands in the run `run`'s pane, drawn in `look`: its keys stand in
    /// the screen's legend and what a reader needs stands above it. A card holds its
    /// question and its options, and under its question why a task asks for the scope it
    /// names, in the task's words, where it is one task's request; a round holds its
    /// questions under its question, the run's handle at its far end, the gist above saying
    /// that nothing is recorded until every one is answered; the plan's desk holds the plan at
    /// the place read, the question and its options at its foot.
    pub(super) fn block(&self, look: &Look, run: &RunId) -> Block {
        let block = match &self.posed {
            Posed::Decision(deciding) => deciding.block(look),
            Posed::Round { round, .. } => round.block(look),
        };
        let mut card = match block {
            Block::Card(card) => card,
            Block::Desk(mut desk) => {
                desk.foot.legend = Legend::new();
                return Block::Desk(desk);
            }
            other => return other,
        };
        match &self.posed {
            Posed::Decision(deciding) => {
                let escalation = &deciding.decision().escalation;
                let why = yunta_render::escalation::why_it_asks(escalation);
                card.about = why.into_iter().collect();
            }
            Posed::Round { .. } => {
                card.about = Vec::new();
                card.edge = Some(run.handle().to_string());
            }
        }
        card.legend = Legend::new();
        Block::Card(card)
    }

    /// The keys it answers to in the screen's legend, in render's words: on a round,
    /// choosing where a list is answered, answering, going back where a question stands
    /// before, and leaving it for later; on the plan's desk, the desk's; on a card while words
    /// are typed, sending or recording them and going back; else choosing, answering, a note,
    /// the plan where the card offers it, and leaving it waiting.
    pub(super) fn legend(&self) -> Legend {
        match &self.posed {
            Posed::Round { round, .. } => {
                let later = PromptAct::LeaveItForLater.said();
                yunta_render::prompt::round::legend(!round.typing(), round.one_before(), later)
            }
            Posed::Decision(deciding) => match deciding.desk_in_front() {
                Some(desk) => desk.legend_on_a_screen(),
                None if deciding.typing() => card::saying_legend(deciding.card().words_asked()),
                None => card::choosing_legend(
                    ChosenWith::Letters,
                    deciding.card().offers_the_plan(),
                    PromptAct::LeaveItWaiting.said(),
                ),
            },
        }
    }

    /// Every key it takes, as help over the run says them: its legend's, then the keys that
    /// legend leaves to help: the plan's desk's, where it stands in front; on a card choosing,
    /// `tab`, `shift-tab` and the number of each option; none on a typed line or a round,
    /// where every key types.
    pub(super) fn keys_for_help(&self) -> Legend {
        let beside = match &self.posed {
            Posed::Round { .. } => Legend::new(),
            Posed::Decision(deciding) => match deciding.desk_in_front() {
                Some(desk) => return desk.keys_for_help(),
                None if deciding.typing() => Legend::new(),
                None => card::left_to_help(deciding.card().offered().len()),
            },
        };
        let Legend(keys) = self.legend();
        Legend(keys.into_iter().chain(beside.0).collect())
    }

    /// Where what is read in it is scrolled to: the pane of the plan's desk, where it stands
    /// in front, else what stands above the card.
    pub(super) fn scrolled(&self) -> &Scroll {
        match &self.posed {
            Posed::Decision(deciding) => deciding
                .desk_in_front()
                .map_or(&self.above, PlanDesk::scroll),
            Posed::Round { .. } => &self.above,
        }
    }

    /// Where what is read in it is scrolled to, for a key that scrolls it or what a surface
    /// laid out of it.
    pub(super) fn scrolled_mut(&mut self) -> &mut Scroll {
        match &mut self.posed {
            Posed::Decision(deciding) => match deciding.desk_in_front_mut() {
                Some(desk) => desk.scroll_mut(),
                None => &mut self.above,
            },
            Posed::Round { .. } => &mut self.above,
        }
    }
}

impl RunStage {
    /// The prompt in place the run `framed` holds in sight: the one held for the pause the
    /// run stands at, where its scene composes a card or a round. None anywhere else.
    pub(super) fn in_place_in<'s>(&'s self, framed: Option<&Framed<'_>>) -> Option<&'s InPlace> {
        let framed = framed?;
        let held = self.in_place.as_deref()?;
        let stands = self.placed_as(framed).is_some();
        (stands && Some(&held.pause) == awaited_pause(framed)).then_some(held)
    }

    /// The prompt in place the run `framed` holds in sight, for a key or a layout.
    fn in_place_in_mut(&mut self, framed: Option<&Framed<'_>>) -> Option<&mut InPlace> {
        let framed = framed?;
        let stands = self.placed_as(framed).is_some();
        let pause = awaited_pause(framed);
        let held = self.in_place.as_deref_mut()?;
        (stands && Some(&held.pause) == pause).then_some(held)
    }

    /// The prompt held for the pause the run `framed` stands at, kept where it belongs to that
    /// pause, whatever news of it come and wherever the person points, built where the scene
    /// composes a card for the decision read for it, or a round for the questions read for
    /// it, and let go where the run stands at another pause or at none, what was typed or
    /// picked on it going with it: noted, where the run's log holds an answer to its pause.
    /// The first prompt built takes where the stage opens.
    pub(super) fn prompt_placed(&mut self, framed: Option<&Framed<'_>>) {
        let Some(framed) = framed else {
            return;
        };
        let pause = awaited_pause(framed);
        let kept = self
            .in_place
            .as_ref()
            .is_some_and(|held| Some(&held.pause) == pause);
        if kept {
            return;
        }
        let gone = self.in_place.take();
        let noted = gone.and_then(|gone| gone.noted_as_it_goes(&framed.in_view.run));
        if let Some(noted) = noted {
            self.notice = Some(noted);
        }
        let built = self.prompt_for(framed);
        if built.is_some() {
            self.opens.take();
        }
        self.in_place = built.map(Box::new);
    }

    /// The prompt the run `framed` would hold for the pause it stands at, with no key pressed
    /// yet: a card or the plan's desk for the decision read for it, a round for the questions
    /// read for it, as its scene composes one; none where it composes none or nothing is read.
    fn prompt_for(&self, framed: &Framed<'_>) -> Option<InPlace> {
        let pause = awaited_pause(framed)?;
        match self.placed_as(framed)? {
            PlacedAs::Card(grounds) => {
                let decision = framed.in_view.awaited_decision()?;
                Some(InPlace::of(&self.run, decision, (pause, grounds)))
            }
            PlacedAs::Desk => {
                let decision = framed.in_view.awaited_decision()?;
                Some(InPlace::desk(&self.run, decision, pause))
            }
            PlacedAs::Round => {
                let asked = framed.in_view.awaited_round(framed.frame)?;
                Some(InPlace::round(asked, pause))
            }
        }
    }

    /// What `stroke` asks for of the prompt in place the run `framed` holds in sight, where
    /// the stroke is its own; none where no prompt stands or the stroke is the stage's.
    pub(super) fn answered_in_place(
        &mut self,
        stroke: Stroke,
        framed: Option<&Framed<'_>>,
    ) -> Option<Vec<Want>> {
        let run = self.run.clone();
        let held = self.in_place_in_mut(framed)?;
        held.takes(stroke).then(|| held.pressed(stroke, &run))
    }

    /// What `esc` does on the prompt the run `framed` holds in sight, where `esc` is not the
    /// prompt's own: it puts the prompt aside with nothing recorded, the person taking the
    /// pointer on the node the run put it on, the node it waits at, whose own scene says what
    /// it waits on; the note says that nothing of a round was recorded. The prompt stays held
    /// for its pause. None where no prompt stands in sight, `esc` doing what it does on the
    /// run.
    pub(super) fn put_aside(&mut self, framed: Option<&Framed<'_>>) -> Option<Vec<Want>> {
        let framed = framed?;
        let notice = self.in_place_in(Some(framed))?.noted_aside();
        let node = self.pointed_in(framed)?;
        self.node = PointedBy::ThePerson(node);
        self.notice = notice;
        Some(Vec::new())
    }

    /// What the run does now that its log recorded the answer given here, noted: nothing
    /// where no answer was under way here.
    pub(super) fn answer_recorded(&mut self) -> Option<String> {
        self.in_place.as_mut().and_then(|held| held.recorded())
    }

    /// The answer under way here handed back, refused or failed.
    pub(super) fn answer_unrecorded(&mut self) {
        if let Some(held) = self.in_place.as_mut() {
            held.unrecorded();
        }
    }

    /// Whether a line is typed on the prompt in place the run `followed` keeps holds in
    /// sight at `now`, as [`RunStage::typing_in`] says.
    pub(crate) fn typing(&self, followed: &FollowedRuns, now: DateTime<Utc>) -> bool {
        self.typing_in(self.framed(followed, now).as_ref())
    }

    /// Whether a line is typed on the prompt in place the run `framed` holds in sight, which
    /// stands only while the run leads: then every key types there. A prompt held for its
    /// pause and out of sight, as a round once the log holds its answer and before anything
    /// says nothing waits, types nothing: what the screen says and what a key does are the
    /// prompt's only where it stands, the one rule [`RunStage::in_place_in`] holds.
    pub(super) fn typing_in(&self, framed: Option<&Framed<'_>>) -> bool {
        self.leads() && self.in_place_in(framed).is_some_and(InPlace::typing)
    }

    /// Whether the plan the run `framed`'s decision is on stands in front of its card: as the
    /// prompt held for the pause the run stands at holds it, or, before the first prompt is
    /// held, as the stage opens, on the plan for a person who came to read it.
    pub(super) fn plan_in_front(&self, framed: &Framed<'_>) -> bool {
        let pause = awaited_pause(framed);
        let held = self.in_place.as_deref();
        match held.filter(|held| Some(&held.pause) == pause) {
            Some(held) => held.plan_in_front(),
            None => self.opens == Some(Opens::ThePlan),
        }
    }

    /// How the scene the run `framed` stands as places a prompt in its pane: as a card
    /// resting on what it rests on, as a round, or as the plan's desk. None where it places
    /// none.
    fn placed_as(&self, framed: &Framed<'_>) -> Option<PlacedAs> {
        match self.scene_of(framed)?.composes() {
            ComposedPane::Card(grounds) => Some(PlacedAs::Card(grounds)),
            ComposedPane::Round => Some(PlacedAs::Round),
            ComposedPane::Desk => Some(PlacedAs::Desk),
            ComposedPane::Work
            | ComposedPane::Board
            | ComposedPane::Findings
            | ComposedPane::Node
            | ComposedPane::Standstill
            | ComposedPane::PullRequest
            | ComposedPane::AskedElsewhere
            | ComposedPane::Receipt
            | ComposedPane::Ended => None,
        }
    }

    /// The prompt the run `framed` holds in sight, as it stands in its pane in `look`: the
    /// prompt held for its pause, or, before one is, the prompt it would be with no key
    /// pressed yet.
    pub(super) fn prompt_in_sight(&self, framed: &Framed<'_>, look: &Look) -> Option<Block> {
        if let Some(held) = self.in_place_in(Some(framed)) {
            return Some(held.block(look, &self.run));
        }
        Some(self.prompt_for(framed)?.block(look, &self.run))
    }
}

/// The pause the run `framed` waits on a person at, as the follower read it: none before it
/// is read or while it waits on none.
fn awaited_pause<'a>(framed: &Framed<'a>) -> Option<&'a AnswerablePause> {
    framed.in_view.awaited.as_ref().map(AwaitedReading::pause)
}
