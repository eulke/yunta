//! The page a gate's decision is shown on, above what answers it: the card of
//! a terminal's prompt, or the decision a pull request ends on. One headline,
//! each document in the form the page shows it, and what weighs on the
//! decision; on a forge, where the decision waits, what the description
//! leaves out and what the run changed as well.
//!
//! Where it is drawn says how much it carries, so the page is one value whose
//! variant is that place. What answers the decision is not the page's: a
//! prompt draws its card under it, and `published` ends a pull request's
//! description on the decision a review gives.

use surco::components::{Fields, Gist, Headline, Note};
use surco::doc::{Block, Doc};
use surco::look::Look;
use surco::words::Standing;
use yunta_core::events::GateWaitingPayload;
use yunta_core::shown::ShownDocument;
use yunta_core::RunId;

use super::{desk, shown, Form, Page, Shown};

/// A gate's decision, as the page a person reads above what answers it,
/// drawn as the place it stands in says. A page with nothing to say is empty:
/// the card opens on its question.
#[derive(Debug, Clone, Copy)]
pub enum Gate<'a> {
    /// Above the decision a pull request ends on, on the forge a reviewer
    /// answers on: the headline at the run's handle, where the decision
    /// waits, what the description leaves out, what the run changed, and each
    /// document as a forge shows it.
    OnTheForge {
        /// The run the decision is in: its handle stands at the headline's
        /// edge, and each command a document names is written for it.
        run: &'a RunId,
        /// Each document the decision is about, as much of it as the
        /// description keeps.
        documents: &'a [ShownDocument],
        /// Where the decision waits, said under the headline.
        waits: &'a str,
        /// What the description leaves out and where it is whole, when it
        /// leaves anything.
        note: Option<&'a str>,
        /// What the run changed from its base to its newest tree, as the fact
        /// the page says it in, when the run's change was counted.
        changed: Option<&'a str>,
    },
    /// What the card of a terminal's prompt stands under: what weighs on the
    /// decision, when a plan among its documents is read at its desk with the
    /// rest of them; else as [`Gate::InPlaceOfTheDesk`].
    AboveTheCard {
        /// The run the decision is in, with each document it is about as the
        /// run holds it; `None` where it is about none.
        shown: Option<(&'a RunId, &'a [ShownDocument])>,
        /// The decision, which says what it keeps from the person.
        escalation: &'a GateWaitingPayload,
    },
    /// What a terminal with no room for a plan's desk prints over the card:
    /// each document as a terminal shows it, under the headline, and what
    /// weighs on the decision last.
    InPlaceOfTheDesk {
        /// The run the decision is in, with each document it is about as the
        /// run holds it; `None` where it is about none.
        shown: Option<(&'a RunId, &'a [ShownDocument])>,
        /// The decision, which says what it keeps from the person.
        escalation: &'a GateWaitingPayload,
    },
}

impl Page for Gate<'_> {
    fn document(&self, look: &Look) -> Doc {
        match *self {
            Gate::OnTheForge {
                run,
                documents,
                waits,
                note,
                changed,
            } => {
                let mut doc = Doc::new()
                    .with(decision_headline().edge(run.handle()))
                    .with(Gist::new(waits));
                if let Some(note) = note {
                    doc = doc.with(Note(note.to_string()));
                }
                if let Some(changed) = changed {
                    doc = doc.with(Fields::new().push_if("changed", changed.to_string()));
                }
                with_documents(doc, (run, documents), Form::Published, look)
            }
            Gate::AboveTheCard { shown, escalation } => {
                let documents = shown.map_or(&[][..], |(_, documents)| documents);
                match desk::plan_among(documents) {
                    None => whole(shown, escalation, look),
                    Some(_) => Doc::new().with_all(
                        shown::before_you_decide(escalation.withheld(), documents)
                            .map(Block::Section),
                    ),
                }
            }
            Gate::InPlaceOfTheDesk { shown, escalation } => whole(shown, escalation, look),
        }
    }
}

/// Everything a decision has to say at a terminal: each document it shows, as a
/// terminal shows it, under its headline, and what weighs on it last.
fn whole(
    shown: Option<(&RunId, &[ShownDocument])>,
    escalation: &GateWaitingPayload,
    look: &Look,
) -> Doc {
    let documents = shown.map_or(&[][..], |(_, documents)| documents);
    let doc = match shown.filter(|(_, documents)| !documents.is_empty()) {
        Some(shown) => with_documents(
            Doc::new().with(decision_headline()),
            shown,
            Form::Review,
            look,
        ),
        None => Doc::new(),
    };
    doc.with_all(shown::before_you_decide(escalation.withheld(), documents).map(Block::Section))
}

/// `doc` with each of `documents`, of the run `run`, drawn after it in `form`.
fn with_documents(
    doc: Doc,
    (run, documents): (&RunId, &[ShownDocument]),
    form: Form,
    look: &Look,
) -> Doc {
    documents.iter().fold(doc, |doc, document| {
        let shown = Shown {
            document,
            run,
            form,
        };
        doc.with_all(shown.document(look).into_blocks())
    })
}

/// The headline every decision's page opens on: the mark of what needs a
/// person, and the word it is called by.
fn decision_headline() -> Headline {
    Headline::new(
        Standing::NeedsYou.mark(),
        "decision",
        Standing::NeedsYou.word(),
    )
}
