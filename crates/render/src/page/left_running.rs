//! What a terminal says when a person stops following a run that goes on:
//! that it runs without this terminal, and the command that follows it again.

use surco::components::{Gist, Headline, Next};
use surco::doc::Doc;
use surco::look::Look;
use surco::words::Standing;
use yunta_core::step::NextStep;
use yunta_core::RunId;

use super::Page;
use crate::command;

/// The run a person stopped following, left running without this terminal,
/// and the command that follows it again.
pub struct LeftRunning<'a> {
    pub run_id: &'a RunId,
}

impl Page for LeftRunning<'_> {
    fn document(&self, _look: &Look) -> Doc {
        let handle = self.run_id.handle();
        Doc::new()
            .with(Headline::new(
                Standing::Running.mark(),
                format!("run {handle}"),
                Standing::Running.word(),
            ))
            .with(Gist::new(
                "this terminal stopped following it; the run goes on without it",
            ))
            .with(Next {
                steps: vec![(
                    command::line(&NextStep::Follow {
                        run: self.run_id.clone(),
                    }),
                    "follows it again",
                )],
            })
    }
}
