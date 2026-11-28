//! What `yunta` alone shows where it has no field to show.

use surco::components::{Gist, Headline};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::{NextStep, Reach};

use super::Page;
use crate::command::{self, SaidStep};

/// What `yunta` alone shows where it has no field to show: that it works in
/// a repository, and the two ways on from here.
pub struct Signpost;

impl Signpost {
    /// The signpost as the application's screen shows it: that yunta works in a repository,
    /// under its headline. The ways on from here are the screen's keys and its palette's
    /// rows, and the printed page's. Not [`Signpost::document`](Page::document), the page
    /// printed where no terminal holds the application.
    pub fn on_screen(&self) -> Doc {
        Doc::new()
            .with(Headline::new(Mark::Absent, "yunta", "not in a repository"))
            .with(Gist::new(
                "this directory is in no git repository, and yunta works in one",
            ))
    }
}

impl Page for Signpost {
    fn document(&self, _look: &Look) -> Doc {
        let machine = NextStep::ListRuns {
            reach: Reach::Machine,
        };
        self.on_screen().with(command::next([
            SaidStep::of(&machine),
            SaidStep::of(&NextStep::Help),
        ]))
    }
}
