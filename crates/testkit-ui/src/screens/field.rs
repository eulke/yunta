//! See [`super`]. The field as a target's cut makes it: the application opened in the cut's
//! repository over every run the target's row of [`TARGET_CUTS`](crate::cut::TARGET_CUTS)
//! shows, loaded into a fresh store ([`Cut::of_target`]), and driven as every helper drives a
//! state: each request carried out and each key pressed once the state waits on nothing and
//! stands caught up with the log, through one following for the screen's life, as the
//! application's does.
//! The run the target points at is pointed by `j`, each press once the one before stands,
//! until the pointed row names it; then the target's keys are pressed, each as the design's
//! recording names a key ([`opened`]). What the application is handed of where the cut's
//! repository stands, and of the runs it cannot read, names the cut's place as the target's
//! frames name it, `/cut`, as the dumps they were written from do: the place is a fresh
//! directory on each machine, and a frame's trail and a run's why would say it. A test that
//! shows the field of runs of its own opens it over a cut it makes ([`over`]).

use std::sync::Arc;

use surco::surface::Markdown;
use yunta_api::FollowNews;
use yunta_core::events::Channel;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, State};

use super::{named_as, strokes, target, CUT};
use crate::cut::{Cut, Pointed, Stands, TargetCut};
use crate::driving::{driven_through, Driven, Inputs};

/// What marks the row a key acts on, as Markdown draws a list a pointer moves along.
const POINTER: char = '▸';

/// The application on the field as a target's cut stands, with the cut it reads and the
/// performer that works there, so a test presses keys on it. Not the field's stage, which
/// the state holds.
pub struct FieldScreen {
    /// The application, as the keys and the runs' news left it.
    state: State,
    /// What carries out its work, in the cut's repository.
    performer: Arc<Performer>,
    /// The cut, kept while the screen is: its store, its runs and its repository.
    cut: Cut,
    /// Where its performer hands back its work and its one following's news, for the
    /// screen's life.
    inputs: Inputs,
}

/// The field of the target `name`: its cut loaded, the application opened in the cut's
/// repository on its field, the run it names pointed by `j`, then its keys pressed.
pub async fn opened(name: &str) -> FieldScreen {
    let target = target(name);
    let Stands::Field(_) = target.stands else {
        panic!("`{name}` is not the field: {:?}", target.stands);
    };
    let cut = Cut::of_target(name).await;
    let place = cut.root().to_path_buf();
    let inputs = Inputs::naming(move |input| named_as(input, &place, CUT));
    let mut screen = on(cut, inputs).await;
    screen.pointed(target).await;
    let keys: Vec<Stroke> = target.keys.iter().flat_map(|key| strokes(key)).collect();
    screen.pressed(&keys).await;
    screen
}

/// The application opened in `cut`'s repository, on the field of the runs the cut loaded,
/// nothing pointed and no key pressed.
pub async fn over(cut: Cut) -> FieldScreen {
    on(cut, Inputs::new()).await
}

/// The application opened in `cut`'s repository, on the field of the runs the cut loaded,
/// what its performer hands back taken through `inputs`.
async fn on(cut: Cut, mut inputs: Inputs) -> FieldScreen {
    let performer = Arc::new(Performer::new(cut.workspace(), Channel::Tty));
    let (opened, asked) = State::opened(cut.now());
    let Driven { state, .. } = driven_through(&mut inputs, opened, asked, &performer, &[]).await;
    FieldScreen {
        state,
        performer,
        cut,
        inputs,
    }
}

impl FieldScreen {
    /// The application as it stands.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// The cut the field stands on: its runs, its repository and its clock.
    pub fn cut(&self) -> &Cut {
        &self.cut
    }

    /// `news` of what the application follows folded in, as its following hands it, and
    /// nothing driven after it: the field stands as the news leaves it until the next key.
    pub fn handed(&mut self, news: FollowNews) {
        let asked = self.state.update(Input::FollowNews(news));
        assert!(asked.is_empty(), "news handed asks for nothing: {asked:?}");
    }

    /// `strokes` pressed, each once the state waits on nothing and stands caught up with the
    /// log.
    pub async fn pressed(&mut self, strokes: &[Stroke]) {
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
    }

    /// The run `target` names pointed by `j`, pressed until the pointed row names it: none
    /// where the pointer already stands on it, and as many times at most as the field lists
    /// runs, which go around to it.
    async fn pointed(&mut self, target: &TargetCut) {
        let Some(pointed) = target.pointed else {
            return;
        };
        let Pointed::Run(handle) = pointed else {
            panic!("`{}` points at a node of a run: {pointed:?}", target.name);
        };
        for _ in 0..self.cut.runs().len() {
            if self.pointed_row().contains(handle) {
                return;
            }
            self.pressed(&[Stroke::Insert('j')]).await;
        }
        assert!(
            self.pointed_row().contains(handle),
            "`{}`: `j` never reached `{handle}`",
            target.name
        );
    }

    /// The row the pointer stands on, as Markdown draws the field.
    fn pointed_row(&self) -> String {
        let screen = Markdown.screen(&self.state.screen(&Markdown.look()));
        let row = screen.lines().find(|line| line.contains(POINTER));
        row.unwrap_or_default().to_string()
    }
}
