//! The page a gate's decision is shown on: one headline, where the decision
//! waits, what the page leaves out, what the run changed, each document in
//! the form the page shows it, and the decision a person answers last. A
//! terminal draws it above the prompt that answers the decision, and a forge
//! draws it as the description of the pull request a reviewer answers on.

use surco::components::{Fields, Gist, Headline, Note, Section};
use surco::doc::Doc;
use surco::words::Standing;
use yunta_core::events::GateWaitingPayload;
use yunta_core::shown::ShownDocument;
use yunta_core::RunId;

use crate::shown::Form;

/// What a gate's page is drawn from, wherever it is drawn: at a terminal
/// above the prompt that answers it, or on a forge's page.
pub struct GatePage<'a> {
    /// The run the decision is in, with each document it is about as the run
    /// holds it: the run's handle stands at the headline's edge, and each
    /// command a document names is written for it. A run with no documents
    /// puts its handle there alone; `None` draws neither.
    pub shown: Option<(&'a RunId, &'a [ShownDocument])>,
    /// Where the decision waits, said under the headline, when the page says it.
    pub waits: Option<String>,
    /// What the page leaves out and where it is whole, when it leaves anything.
    pub note: Option<String>,
    /// What the run changed from its base to its newest tree, as the fact
    /// the page says it in, when the run's change was counted.
    pub changed: Option<String>,
    /// How much of each document the page shows.
    pub form: Form,
    /// The decision the page ends on.
    pub decision: Section,
}

/// The page: one headline, then where it waits, what it leaves out, what the
/// run changed, each document in its form, and the decision last.
pub fn page(page: GatePage<'_>) -> Doc {
    let headline = decision_headline();
    let headline = match page.shown {
        Some((run, _)) => headline.edge(run.handle()),
        None => headline,
    };
    let mut doc = Doc::new().with(headline);
    if let Some(waits) = page.waits {
        doc = doc.with(Gist::new(waits));
    }
    if let Some(note) = page.note {
        doc = doc.with(Note(note));
    }
    if let Some(changed) = page.changed {
        doc = doc.with(Fields::new().push_if("changed", changed));
    }
    if let Some((run, documents)) = page.shown {
        for document in documents {
            let drawn = crate::shown::document(document, run, page.form);
            doc = doc.with_all(drawn.into_blocks());
        }
    }
    doc.with(page.decision)
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

/// Everything a decision has to say as one page: each document it shows, in the
/// form a terminal shows it, under its headline, and what weighs on it last. What
/// a terminal with no room for a desk prints above the card. `None` where the
/// card says it all.
pub fn whole_page(
    shown: Option<(&RunId, &[ShownDocument])>,
    escalation: &GateWaitingPayload,
) -> Option<Doc> {
    let documents: &[ShownDocument] = match shown {
        Some((_, documents)) => documents,
        None => &[],
    };
    let before = crate::shown::before_you_decide(escalation.withheld(), documents);
    if documents.is_empty() && before.is_none() {
        return None;
    }
    let mut doc = Doc::new();
    if let Some((run, documents)) = shown.filter(|(_, documents)| !documents.is_empty()) {
        doc = doc.with(decision_headline());
        for document in documents {
            let drawn = crate::shown::document(document, run, Form::Review);
            doc = doc.with_all(drawn.into_blocks());
        }
    }
    if let Some(before) = before {
        doc = doc.with(before);
    }
    Some(doc)
}

/// What a prompt's card stands under: everything the decision has to say,
/// except a plan among its documents, which is read at its desk, and with it
/// the rest of them. `None` where the card says it all.
pub fn above(
    shown: Option<(&RunId, &[ShownDocument])>,
    escalation: &GateWaitingPayload,
) -> Option<Doc> {
    let documents: &[ShownDocument] = shown.map_or(&[], |(_, documents)| documents);
    match crate::shown::plan_among(documents) {
        None => whole_page(shown, escalation),
        Some(_) => crate::shown::before_you_decide(escalation.withheld(), documents)
            .map(|before| Doc::new().with(before)),
    }
}
