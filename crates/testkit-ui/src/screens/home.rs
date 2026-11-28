//! See [`super`]. A screen a target opens in a place of the test home, as its row of
//! [`TARGET_CUTS`](crate::cut::TARGET_CUTS) names the place: the place made alone under a
//! root of its own, or in a cut of its own where it holds runs ([`HomePlace::made_alone`]),
//! the application opened there, or on the workflows over where it stands where the target
//! says so, and driven as every helper drives a state, each request
//! carried out and each key pressed once the state waits on nothing and stands caught up with
//! the log, through one following for the screen's life, as the application's does; then the
//! target's keys pressed, each as the design's recording names a key ([`opened`]). What the
//! application is handed of where the place's repository stands names the directory the place
//! keeps its state under as the target's frames name the home's root, `/home`, as the dumps
//! they were written from do: it is a fresh directory on each machine, and a frame's trail
//! would say it.

use std::sync::Arc;

use tempfile::TempDir;
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_testkit_core::fixed_now;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Performer, State};

use super::{named_as, strokes, target};
use crate::cut::{HomePlace, Opens, Stands};
use crate::driving::{driven_through, Driven, Inputs};
use crate::home::{Figure, MadeAlone};

/// What a target names the home's root, as the dumps it was written from name it.
const ROOT: &str = "/home";

/// The application at a place of the test home as a target opens there, with the root the
/// place was made under, kept while the screen is. Not the place, which the root holds.
pub struct HomeScreen {
    /// The application, as the keys and what it read there left it.
    state: State,
    /// The place, kept while the screen is: its cut, where it stands in one.
    place: MadeAlone,
    /// Where the place was made.
    _root: TempDir,
}

/// The screen of the target `name`: the place it opens in made alone, the application opened
/// there, then the target's keys pressed.
pub async fn opened(name: &str) -> HomeScreen {
    let target = target(name);
    let Stands::Home(place) = target.stands else {
        panic!("`{name}` opens on a cut: {:?}", target.stands);
    };
    let root = tempfile::tempdir().expect("a root for the place");
    let made_under = root.path().canonicalize().expect("the place's root");
    let made = HomePlace::made_alone(place, &made_under).await;
    let kept_under = made.root.clone();
    let mut inputs = Inputs::naming(move |input| named_as(input, &kept_under, ROOT));
    let performer = Arc::new(Performer::new(made.workspace.clone(), Channel::Tty));
    let (opened, asked) = match target.opens {
        Opens::Bare => State::opened(fixed_now()),
        Opens::OnTheWorkflows => State::opened_on(fixed_now(), &NextStep::List)
            .expect("the workflows' stage takes the step that lists them"),
    };
    let keys: Vec<Stroke> = target.keys.iter().flat_map(|key| strokes(key)).collect();
    let Driven { state, .. } = driven_through(&mut inputs, opened, asked, &performer, &keys).await;
    HomeScreen {
        state,
        place: made,
        _root: root,
    }
}

impl HomeScreen {
    /// The application as it stands.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// What the place made afresh on this machine that the target's frames hold a placeholder
    /// for, as the screen says it.
    pub fn figures(&self) -> &[Figure] {
        &self.place.figures
    }
}
