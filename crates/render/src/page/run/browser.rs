//! A run's page as a browser reads it: the page in its browser scope, and,
//! where the run waits on a decision, each document the decision is about,
//! whole, before it — a browser has the room a terminal does not — and when
//! the page was drawn, under it.

use chrono::{DateTime, Utc};
use surco::components::Note;
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::step::NextStep;

use super::{Run, RunScope};
use crate::command;
use crate::page::{Form, Page, Shown};

/// A run's page as `yunta open` writes it for a browser, drawn at an
/// instant. Built by [`Run::in_a_browser`], which draws the run in its
/// browser scope.
pub struct RunInABrowser<'a> {
    run: Run<'a>,
    drawn_at: DateTime<Utc>,
}

impl<'a> Run<'a> {
    /// The page as a browser reads it, drawn at `drawn_at`.
    pub fn in_a_browser(mut self, drawn_at: DateTime<Utc>) -> RunInABrowser<'a> {
        self.scope = RunScope::Browser;
        RunInABrowser {
            run: self,
            drawn_at,
        }
    }
}

impl Page for RunInABrowser<'_> {
    fn document(&self, look: &Look) -> Doc {
        let run = &self.run;
        let doc = run.document(look);
        let at = doc.decision_at();
        let mut blocks = doc.into_blocks();
        if let Some(decision) = run.decision {
            let at = at.unwrap_or(blocks.len());
            let shown = decision.shown.iter().flat_map(|document| {
                let shown = Shown {
                    document,
                    run: run.run_id,
                    form: Form::Browser,
                };
                shown.document(look).into_blocks()
            });
            blocks.splice(at..at, shown.collect::<Vec<_>>());
        }
        blocks.push(
            Note(format!(
                "drawn at {}; `{}` draws it again.",
                self.drawn_at.format(crate::INSTANT_FORMAT),
                command::line(&NextStep::Open {
                    run: run.run_id.clone()
                }),
            ))
            .into(),
        );
        Doc::new().with_all(blocks)
    }
}
