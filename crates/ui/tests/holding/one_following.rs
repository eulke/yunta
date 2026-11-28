//! See [`super`]. Why a held prompt never meets its run read whole again: the application
//! follows through one performer and one channel of inputs, as a terminal does, and its
//! performer keeps one following for them, which reads the run a screen shows whole once and
//! hands what comes after as events. Pages opened over the run's screen and closed leave what
//! is followed as it was, and the following asked for the same again follows on, so the
//! run's next event comes as events and a pick on its card stands. Only a run's screen moving
//! onto another run, or a new channel of inputs, follows a run anew, as each drive of the
//! test kit does with one of its own.
//!
//! Reached by: `crates/ui/src/performer/following.rs`, `crates/api/src/follow/following.rs`,
//! `crates/ui/src/followed_runs.rs`, `crates/ui/src/state/followed.rs`,
//! `crates/tui/src/application.rs`

use std::sync::Arc;
use std::time::Duration;

use surco::components::CardBody;
use surco::doc::Block;
use surco::look::Look;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yunta_api::FollowNews;
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::RunId;
use yunta_testkit_ui::cut::{Cut, Shown, Stands, TARGET_CUTS};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

use super::THE_GATE_WAITS;

/// How long the application takes at most to stand caught up with the log: its following
/// looks a tenth of a second apart and reads what a parked run asks beside, so only one
/// that never catches up waits half a minute.
const CATCHING_UP: Duration = Duration::from_secs(30);

/// The application on a target's cut as a terminal holds it: one state, one performer, one
/// channel its performer hands every answer and piece of news on, and a count of what it
/// asked to follow and of the times the run it shows came whole.
struct OneChannel {
    state: State,
    performer: Arc<Performer>,
    inputs: UnboundedSender<Input>,
    received: UnboundedReceiver<Input>,
    /// The cut it stands on, kept while it does: its store and its runs.
    cut: Cut,
    /// The run the cut shows, as its target's row cuts it.
    shown: Shown,
    /// How many times the state asked its performer to follow.
    follows_asked: usize,
    /// How many times its following handed the run the cut shows whole.
    wholes: usize,
}

impl OneChannel {
    /// The application opened on the run of the target `name`, at its cut, followed until
    /// it stands caught up with the log.
    async fn opened(name: &str) -> OneChannel {
        let target = TARGET_CUTS.iter().find(|target| target.name == name);
        let Some(Stands::Run(shown)) = target.map(|target| target.stands) else {
            panic!("`{name}` is a run's screen");
        };
        let cut = Cut::of_target(name).await;
        let performer = Arc::new(Performer::new(cut.workspace(), Channel::Tty));
        let (inputs, received) = mpsc::unbounded_channel();
        let run = cut.runs()[0].clone();
        let opened = State::opened_on(cut.now(), &NextStep::status(&run));
        let (state, asked) = opened.expect("the run's stage takes the step");
        let mut application = OneChannel {
            state,
            performer,
            inputs,
            received,
            cut,
            shown,
            follows_asked: 0,
            wholes: 0,
        };
        application.settled(asked).await;
        application
    }

    /// The run the cut shows.
    fn run(&self) -> &RunId {
        &self.cut.runs()[0]
    }

    /// `asked` carried out by the performer on the one channel, and every input it hands
    /// back taken, until the state waits on nothing and stands caught up with the log.
    async fn settled(&mut self, mut asked: Vec<Request>) {
        loop {
            for request in asked.drain(..) {
                self.follows_asked += usize::from(matches!(request, Request::Follow(_)));
                self.performer.take(request, &self.inputs);
            }
            let project = self.cut.workspace().project().expect("the cut's project");
            let heads = yunta_testkit::heads(&project.storage_path);
            if self.state.waiting_on().is_empty() && self.state.caught_up(&heads) {
                return;
            }
            let input = tokio::time::timeout(CATCHING_UP, self.received.recv()).await;
            let input = input
                .expect("the application catches up")
                .expect("an input");
            if let Input::FollowNews(FollowNews::Whole { run, .. }) = &input {
                self.wholes += usize::from(run.run() == self.run());
            }
            asked = self.state.update(input);
        }
    }

    /// `stroke` pressed, and what it asks carried out until the state stands caught up.
    async fn pressed(&mut self, stroke: Stroke) {
        let asked = self.state.update(Input::Stroke(stroke));
        self.settled(asked).await;
    }

    /// The run gone on to `seq`, its next events loaded into the store as the engine that
    /// drives it writes them, and the state followed on until it stands caught up.
    async fn went_on(&mut self, seq: u64) {
        let project = self.cut.workspace().project().expect("the cut's project");
        let heads = yunta_testkit::heads(&project.storage_path);
        let head = heads.get(self.run()).map_or(0, |head| head.get());
        self.cut.goes_on(self.shown, head, seq).await;
        self.settled(Vec::new()).await;
    }

    /// The option the card standing at the foot of the run's pane points at.
    fn pointed(&self) -> Option<String> {
        let main = self.state.screen(&Look::plain()).main;
        let Some(Block::Card(card)) = main.doc.blocks().last() else {
            return None;
        };
        match &card.body {
            CardBody::Choosing { choices, .. } => {
                choices.rows.get(choices.at).map(|row| row.name.clone())
            }
            CardBody::Saying { .. } | CardBody::Round(_) => None,
        }
    }
}

#[tokio::test]
async fn a_runs_screen_is_read_whole_once_so_its_card_keeps_its_pick_through_pages_and_news() {
    let mut application = OneChannel::opened("ship").await;
    assert_eq!(
        application.wholes, 1,
        "the run shown is read whole as it opens"
    );
    let follows = application.follows_asked;
    application.pressed(Stroke::Insert('j')).await;
    assert_eq!(application.pointed().as_deref(), Some("abort"));

    // Its numbers opened over it and closed: what is followed stays as it was.
    application.pressed(Stroke::Insert('s')).await;
    application.pressed(Stroke::Decline).await;
    assert_eq!(
        application.follows_asked, follows,
        "no following asked anew"
    );

    // The same asked again of the performer, as a state asks each change: followed on.
    let followed = application.state.followed();
    application.settled(vec![Request::Follow(followed)]).await;

    application.went_on(THE_GATE_WAITS).await;
    assert_eq!(application.wholes, 1, "what came after came as events");
    assert_eq!(
        application.pointed().as_deref(),
        Some("abort"),
        "the pick on the card stands"
    );
}
