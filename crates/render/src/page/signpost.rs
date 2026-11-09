//! What `yunta` alone shows where it has no field to show.

use surco::components::{Gist, Headline, Next};
use surco::doc::Doc;
use surco::glyphs::Mark;
use surco::look::Look;
use yunta_core::step::{NextStep, Reach};

use super::Page;
use crate::command;

/// What `yunta` alone shows where it has no field to show: that it works in
/// a repository, and the two ways on from here.
pub struct Signpost;

impl Page for Signpost {
    fn document(&self, _look: &Look) -> Doc {
        Doc::new()
            .with(Headline::new(Mark::Absent, "yunta", "not in a repository"))
            .with(Gist::new(
                "this directory is in no git repository, and yunta works in one",
            ))
            .with(Next {
                steps: vec![
                    (
                        command::line(&NextStep::ListRuns {
                            reach: Reach::Machine,
                        }),
                        "every run on this machine",
                    ),
                    (command::line(&NextStep::Help), "what yunta does"),
                ],
            })
    }
}
