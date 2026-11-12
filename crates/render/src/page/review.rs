//! What `yunta review` shows off a terminal: the plan a run waits on, read in
//! full the way a terminal reads it, with the commands that answer its
//! decision at its end.

use surco::components::{Fields, Section};
use surco::doc::Doc;
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_core::events::GateWaitingPayload;
use yunta_core::shown::ShownDocument;
use yunta_core::RunId;

use super::{Form, Page, Shown};
use crate::command;

/// The plan a run waits on, read in full the way a terminal reads it, with the commands
/// that answer the decision at its end: what a person off a terminal reads.
pub struct Review<'a> {
    pub plan: &'a ShownDocument,
    pub run_id: &'a RunId,
    /// The decision the plan waits on.
    pub escalation: &'a GateWaitingPayload,
}

impl Page for Review<'_> {
    fn document(&self, look: &Look) -> Doc {
        let Review {
            plan,
            run_id,
            escalation,
        } = *self;
        let answers = command::answering(run_id, escalation)
            .into_iter()
            .fold(Fields::new(), |fields, (_, command)| {
                fields.push_command("", command)
            });
        let shown = Shown {
            document: plan,
            run: run_id,
            form: Form::Review,
        };
        shown.document(look).with(Section {
            lead: None,
            title: Line::new().push(Tone::Strong, "answer it with"),
            blocks: vec![answers.into()],
        })
    }
}
