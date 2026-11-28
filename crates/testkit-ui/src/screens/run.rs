//! See [`super`]. A run's screen as a target's cut makes it: the application opened on the
//! run the target shows, in its field, the cut its row of
//! [`TARGET_CUTS`](crate::cut::TARGET_CUTS) names loaded
//! into a fresh store ([`Cut::of_target`]), and driven as every helper drives a state:
//! each request carried out and each key pressed once the state waits on nothing and
//! stands caught up with the log. Every drive of a screen goes through the same inputs, so
//! its performer follows the run through one following for the screen's life, as the
//! application's does: what the run asks is read once a pause, and never read whole again
//! while a key's answer lands. The node the person points at is pointed by `j`, each
//! press once the one before stands, then the target's keys are pressed, each as the
//! design's recording names a key ([`opened`]). Where the cut says the run's log goes on
//! once the keys are pressed, its events up to there are loaded then, as a run that moves
//! hands them. Where the cut says what the run asks is not handed yet, it is taken back once
//! the keys are pressed, as the following hands a run that waits on none, so the state keeps
//! none of it.
//!
//! Where a cut's log goes on once its keys are pressed and a key answers the decision the run
//! waits on, the answer is the recording's own: its events from the cut on are loaded in place
//! of what the performer would write, the recording's answer among them, and the answer is
//! handed back as recorded, so the screen notes an answer given here over a log that reads as
//! the recording's.
//!
//! What the run does after its cut reaches the screen as its following reads it: its next
//! events loaded into the cut's store after what a key wrote there, an answer the person
//! recorded among them, and the run driven by the engine it was handed on to
//! ([`RunScreen::went_on`]), or handed in one batch as a following that holds the run hands
//! what came ([`RunScreen::followed_on_to`]); a decision or a round answered elsewhere,
//! through the API as another process records it ([`RunScreen::decided_elsewhere`],
//! [`RunScreen::answered_elsewhere`]); or a piece of its news
//! handed as it is, folded in with nothing driven after it ([`RunScreen::handed`]). What a
//! surface laid out of the screen is handed back as a surface hands it
//! ([`RunScreen::laid`]), for the keys that scroll to move from. A target whose keys scroll
//! is opened laid out before each key ([`opened_laid_out`]), as a terminal draws the screen
//! between two keys: a key that scrolls moves from what the last drawing laid out. Each
//! piece of work the application handed its performer is kept in the order asked, for a
//! test to count a reading that leaves no trace of its own ([`RunScreen::performed`]). What
//! the application is handed of the place the cut is kept under names it as the target's
//! frames name it, `/cut` (`named_as`), as where a run's page for a browser was written.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use surco::components::ScreenLaid;
use surco::look::Look;
use yunta_api::{FollowNews, Project, RunNews};
use yunta_core::events::Channel;
use yunta_core::port::QuestionsReply;
use yunta_core::step::NextStep;
use yunta_core::{Answer, NodeId, OptionId, QuestionId, Responder, RunId, Seq};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Address, Input, Operation, Outcome, Performer, Request, State, Ticket};

use super::{named_as, strokes, target, CUT};
use crate::cut::{Asked, Cut, Pointed, Shown, Stands, TargetCut};
use crate::driving::{driven_through, pressed_while, Driven, Inputs, Withheld};

/// The application on a run's screen as a target's cut stands, with the cut it reads and
/// the performer that works there, so a test presses keys on it and lets the run go on.
/// Not the run's stage, which the state holds.
pub struct RunScreen {
    /// The application, as the keys and the run's news left it.
    state: State,
    /// What carries out its work, in the cut's repository.
    performer: Arc<Performer>,
    /// The run the screen shows, as its target's cut shows it.
    shown: Shown,
    /// The seq the run stands at in the cut's store.
    at: u64,
    /// The cut, kept while the screen is: its store and its runs.
    cut: Cut,
    /// Each piece of work the application handed its performer since it opened, in the
    /// order asked.
    performed: Vec<Operation>,
    /// Where its performer hands back its work and its one following's news, for the
    /// screen's life.
    inputs: Inputs,
}

/// The run's screen of the target `name`: its cut loaded, the application opened on the run
/// it shows, the node it names pointed by `j`, its keys pressed, an answer they give played
/// as the recording recorded it where the cut's log goes on, and what the run asks taken back
/// where the cut says it is not handed yet.
pub async fn opened(name: &str) -> RunScreen {
    let target = target(name);
    let mut screen = at_its_cut(target).await;
    let keys: Vec<Stroke> = target.keys.iter().flat_map(|key| strokes(key)).collect();
    match target.goes_on_to {
        Some(seq) => {
            if !screen.pressed_answering_as_recorded(&keys, seq).await {
                screen.gone_on(target).await;
            }
        }
        None => {
            screen.pressed(&keys).await;
        }
    }
    screen.asked(target);
    screen
}

/// The run's screen of the target `name` as [`opened`] opens it, laid out by `laid_out`
/// before each of its keys, as a surface lays the screen out between two keys: what it
/// hands back is what the next key moves from.
pub async fn opened_laid_out(
    name: &str,
    mut laid_out: impl FnMut(&State) -> ScreenLaid,
) -> RunScreen {
    let target = target(name);
    let mut screen = at_its_cut(target).await;
    for stroke in target.keys.iter().flat_map(|key| strokes(key)) {
        let laid = laid_out(screen.state());
        screen.laid(laid);
        screen.pressed(&[stroke]).await;
    }
    screen.gone_on(target).await;
    screen.asked(target);
    screen
}

/// The run's screen of `target` at its cut: the cut loaded, the application opened on the
/// run it shows and the node it names pointed by `j`, no key of its own pressed yet.
async fn at_its_cut(target: &TargetCut) -> RunScreen {
    let Stands::Run(shown) = target.stands else {
        panic!(
            "`{}` is not a run's screen: {:?}",
            target.name, target.stands
        );
    };
    let cut = Cut::of_target(target.name).await;
    let performer = Arc::new(Performer::new(cut.workspace(), Channel::Tty));
    let run = cut.runs()[0].clone();
    let (opened, asked) = State::opened_on(cut.now(), &NextStep::status(&run))
        .expect("the run's stage takes the step");
    let place = cut.root().to_path_buf();
    let mut inputs = Inputs::naming(move |input| named_as(input, &place, CUT));
    let Driven {
        state, performed, ..
    } = driven_through(&mut inputs, opened, asked, &performer, &[]).await;
    let mut screen = RunScreen {
        state,
        performer,
        shown,
        at: shown.seq,
        cut,
        performed,
        inputs,
    };
    screen.pointed(target).await;
    screen
}

impl RunScreen {
    /// The run gone on to where `target`'s cut says its log goes on to once the keys are
    /// pressed, driven by the engine it was handed on to; nothing where the keys leave the
    /// log as it stands.
    async fn gone_on(&mut self, target: &TargetCut) {
        if let Some(seq) = target.goes_on_to {
            self.cut.driven(self.shown);
            self.went_on(seq).await;
        }
    }

    /// What the run asks taken back where `target`'s cut says it is not handed yet.
    fn asked(&mut self, target: &TargetCut) {
        match target.asked {
            Asked::Handed => {}
            Asked::NotYet => self.handed(RunNews::Awaited(None)),
        }
    }

    /// `strokes` pressed, each once the state stands at rest, where the run's log goes on to
    /// `seq` as the recording's did: an answer a stroke asks to record is the recording's
    /// own, its events from the cut on to `seq` loaded in place of what the performer would
    /// write, the answer among them as the recording's log holds it, and handed back as
    /// recorded; whether a stroke answered so.
    async fn pressed_answering_as_recorded(&mut self, strokes: &[Stroke], seq: u64) -> bool {
        let mut answered = false;
        for stroke in strokes {
            let mut rest = Vec::new();
            for request in self.state.update(Input::Stroke(*stroke)) {
                let Request::Perform(ticket, Operation::Decide { run, .. }) = request else {
                    rest.push(request);
                    continue;
                };
                self.cut.driven(self.shown);
                self.cut.goes_on(self.shown, self.at, seq).await;
                self.at = seq;
                answered = true;
                let decided = Outcome::Decided { run };
                rest.extend(self.state.update(Input::Outcome(ticket, decided)));
            }
            let state = self.state.clone();
            let driven = driven_through(&mut self.inputs, state, rest, &self.performer, &[]).await;
            self.state = driven.state;
            self.performed.extend(driven.performed);
        }
        answered
    }

    /// The application as it stands.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// `strokes` pressed, each once the state waits on nothing and stands caught up with the
    /// log: what only a surface does that the state asked for meanwhile, showing a page or
    /// leaving.
    pub async fn pressed(&mut self, strokes: &[Stroke]) -> Vec<Request> {
        let state = self.state.clone();
        let driven = driven_through(
            &mut self.inputs,
            state,
            Vec::new(),
            &self.performer,
            strokes,
        )
        .await;
        self.state = driven.state;
        self.performed.extend(driven.performed);
        driven.surfaced
    }

    /// `strokes` pressed while the work `withheld` picks is under way, each once the state
    /// waits on nothing else and stands caught up with the log, that work held back from the
    /// performer ([`pressed_while`]): each piece of it under its ticket, which the state still
    /// waits on where no key let it go, for the test to answer ([`RunScreen::answered`]).
    pub async fn pressed_while(
        &mut self,
        withheld: impl Fn(&Operation) -> bool,
        strokes: &[Stroke],
    ) -> Vec<(Ticket, Operation)> {
        let state = self.state.clone();
        let performer = &self.performer;
        let pressed = pressed_while(
            &mut self.inputs,
            state,
            Vec::new(),
            performer,
            withheld,
            strokes,
        );
        let Withheld { driven, withheld } = pressed.await;
        self.state = driven.state;
        self.performed.extend(driven.performed);
        withheld
    }

    /// `outcome` handed back under `ticket`, as the performer hands back what work came to,
    /// and the state driven until it waits on nothing and stands caught up with the log.
    pub async fn answered(&mut self, ticket: Ticket, outcome: Outcome) {
        let asked = self.state.update(Input::Outcome(ticket, outcome));
        let state = self.state.clone();
        let driven = driven_through(&mut self.inputs, state, asked, &self.performer, &[]).await;
        self.state = driven.state;
        self.performed.extend(driven.performed);
    }

    /// Each piece of work the application handed its performer since it opened, in the
    /// order asked: what a reading that leaves no trace of its own is counted by.
    pub fn performed(&self) -> &[Operation] {
        &self.performed
    }

    /// How many times the application polled the forge the run's gate was published on.
    pub fn forge_polls(&self) -> usize {
        self.cut.forge_polls()
    }

    /// The directory the run the screen shows is kept in, beside its log: where its receipt
    /// would stand once written.
    pub fn run_dir(&self) -> PathBuf {
        let run = &self.cut.runs()[0];
        self.project()
            .run_dir(run.as_str())
            .expect("the run shown is kept in the cut's runs")
    }

    /// Each run's last seq in the cut's store, as its log stands now: what a key that
    /// answers a run would move.
    pub fn heads(&self) -> BTreeMap<RunId, Seq> {
        yunta_testkit::heads(&self.store())
    }

    /// Where the cut keeps every run's log, what a key that answers a run writes to among
    /// them.
    pub fn store(&self) -> PathBuf {
        self.project().storage_path
    }

    /// `news` of the run the screen shows folded in, as its following hands it, and nothing
    /// driven after it: the screen stands as the news leaves it until the next key or the
    /// run's next event, which drive the following again.
    pub fn handed(&mut self, news: RunNews) {
        let run = self
            .state
            .followed()
            .run
            .expect("a run's screen follows its run");
        let asked = self
            .state
            .update(Input::FollowNews(FollowNews::Run { run, news }));
        assert!(asked.is_empty(), "news handed asks for nothing: {asked:?}");
    }

    /// What the surface was asked to show and could not, `address`, handed back as a surface
    /// hands it.
    pub fn unshown(&mut self, address: Address) {
        let asked = self.state.update(Input::Unshown(address));
        assert!(
            asked.is_empty(),
            "what was not shown asks for nothing: {asked:?}"
        );
    }

    /// What a surface laid out of the screen, `laid`, handed back as a surface hands it after
    /// it draws, for the keys that scroll to move from.
    pub fn laid(&mut self, laid: ScreenLaid) {
        let asked = self.state.update(Input::Laid(laid));
        assert!(asked.is_empty(), "a layout asks for nothing: {asked:?}");
    }

    /// The run gone on to `seq`: its events past where its log stands loaded into the cut's
    /// store, after what a key the person pressed wrote there, and the state driven until it
    /// stands caught up with them.
    pub async fn went_on(&mut self, seq: u64) {
        let run = &self.cut.runs()[0];
        let head = self.heads().get(run).map_or(self.at, |head| head.get());
        self.cut.goes_on(self.shown, head.max(self.at), seq).await;
        self.at = seq;
        self.pressed(&[]).await;
    }

    /// The run gone on to `seq` as the following that holds it hands what came: its events
    /// past where its log stands loaded into the cut's store, then handed in one batch and
    /// folded as such a following hands them, and the state driven until it stands caught up
    /// with them, what the run asks once it parks read as the following reads it. Not
    /// [`RunScreen::went_on`], whose events come in the batches the screen's following finds
    /// at its looks.
    pub async fn followed_on_to(&mut self, seq: u64) {
        let run = self
            .state
            .followed()
            .run
            .expect("a run's screen follows its run");
        let shown = &self.cut.runs()[0];
        let head = self.heads().get(shown).map_or(self.at, |head| head.get());
        let came = self.cut.goes_on(self.shown, head.max(self.at), seq).await;
        self.at = seq;
        let news = FollowNews::Run {
            run,
            news: RunNews::Events(came),
        };
        let asked = self.state.update(Input::FollowNews(news));
        let state = self.state.clone();
        let driven = driven_through(&mut self.inputs, state, asked, &self.performer, &[]).await;
        self.state = driven.state;
        self.performed.extend(driven.performed);
    }

    /// The decision the run waits on answered elsewhere, `option` chosen by `by`, as another
    /// process records it through the API (`yunta resolve-gate` at a shell), the run handed
    /// on to the engine the cut hands runs to; then the state driven until it stands caught
    /// up with what that wrote.
    pub async fn decided_elsewhere(&mut self, option: &str, by: &str) {
        let by: Responder = by.parse().expect("a responder's name");
        let project = self.project();
        let opened = project.run(&self.cut.runs()[0]).await;
        let opened = opened.expect("the run the screen shows");
        let decided = opened.decide(OptionId::from(option), Some(&by), None);
        decided.await.expect("the decision is recorded elsewhere");
        self.pressed(&[]).await;
    }

    /// The round of questions the node `node` asks answered elsewhere, each question's id
    /// with its answer, by `by` through the run's tools (`Channel::Mcp`), as another process
    /// records it through the API, the run handed on to the engine the cut hands runs to;
    /// then the state driven until it stands caught up with what that wrote.
    pub async fn answered_elsewhere(&mut self, node: &str, answers: &[(&str, &str)], by: &str) {
        let responder: Responder = by.parse().expect("a responder's name");
        let answers = answers.iter().map(|(id, value)| Answer {
            id: QuestionId::from(*id),
            value: (*value).to_string(),
        });
        let reply = QuestionsReply {
            answers: answers.collect(),
            channel: Channel::Mcp,
            responder: Some(responder),
        };
        let project = self.project();
        let opened = project.run(&self.cut.runs()[0]).await;
        let opened = opened.expect("the run the screen shows");
        let node = NodeId::from(node);
        let answered = opened.answer(&node, reply);
        answered.await.expect("the answers are recorded elsewhere");
        self.pressed(&[]).await;
    }

    /// The project the cut's workspace works in, as another process on this machine opens
    /// it: where its runs are kept and the store they log in.
    fn project(&self) -> Project {
        let project = self.performer.workspace().project();
        project.expect("the cut's workspace names its project")
    }

    /// The node `target` names pointed by `j`, pressed until the trail names it: once at
    /// least, so the person holds the pointer even where the run put it there, and as many
    /// times at most as the run has nodes, which go around to it.
    async fn pointed(&mut self, target: &TargetCut) {
        let Some(pointed) = target.pointed else {
            return;
        };
        let Pointed::Node(node) = pointed else {
            panic!(
                "`{}` points at a run of the field: {pointed:?}",
                target.name
            );
        };
        let in_view = self.state.followed_runs().in_view.as_ref();
        let nodes = in_view.map_or(0, |in_view| in_view.run.frame().nodes.len());
        for _ in 0..nodes {
            self.pressed(&[Stroke::Insert('j')]).await;
            if self.named().as_deref() == Some(node) {
                return;
            }
        }
        assert_eq!(
            self.named().as_deref(),
            Some(node),
            "`{}`: `j` never reached it",
            target.name
        );
    }

    /// The node the trail names after the run's handle, where it names one.
    fn named(&self) -> Option<String> {
        let trail = self.state.screen(&Look::plain()).trail;
        let [_, _, node] = trail.as_slice() else {
            return None;
        };
        Some(node.clone())
    }
}
