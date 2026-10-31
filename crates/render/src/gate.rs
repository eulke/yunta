//! The page a gate's decision is shown on: one headline, where the decision
//! waits, what the page leaves out, each document in the form the page
//! shows it, and the decision a person answers last. A terminal draws it
//! above the prompt that answers the decision, and a forge draws it as the
//! description of the pull request a reviewer answers on.

use surco::components::{Gist, Headline, Lead, Note, Section};
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use surco::words::Standing;
use yunta_core::events::GateWaitingPayload;
use yunta_core::shown::ShownDocument;
use yunta_core::step::NextStep;
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
    /// How much of each document the page shows.
    pub form: Form,
    /// The decision the page ends on.
    pub decision: Section,
}

/// The page: one headline, then where it waits, what it leaves out, each
/// document in its form, and the decision last.
pub fn page(page: GatePage<'_>, spell: &dyn Fn(&NextStep) -> String) -> Doc {
    let headline = Headline::new(
        Standing::NeedsYou.mark(),
        "decision",
        Standing::NeedsYou.word(),
    );
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
    if let Some((run, documents)) = page.shown {
        for document in documents {
            let drawn = crate::shown::document(document, run, page.form, spell);
            doc = doc.with_all(drawn.into_blocks());
        }
    }
    doc.with(page.decision)
}

/// A decision as the section a page ends on: led by the mark of what
/// needs a person, and titled `title`.
pub fn decision(title: impl Into<String>, blocks: Vec<Block>) -> Section {
    let title: String = title.into();
    Section {
        lead: Some(Lead::Mark(Standing::NeedsYou.mark())),
        title: Line::new().push(Tone::Strong, title),
        blocks,
    }
}

/// The record a decision draws beside the section `before_you_decide` draws
/// over `shown`: the facts the engine attached, less the reasons that section
/// names. Each reason is said once: under that section when it names them,
/// and in the record when no plan on the page does.
pub fn record(escalation: &GateWaitingPayload, shown: &[ShownDocument]) -> Vec<String> {
    match crate::shown::says_why(escalation.withheld(), shown) {
        true => crate::escalation::noted(escalation),
        false => crate::escalation::evidence(escalation),
    }
}

/// The decision a prompt asks, as the section its page ends on: titled by
/// its question, then the record the engine attached, what weighs on it,
/// and each option with what it does and costs. No option names a command:
/// the prompt under it is how it is chosen.
pub fn asked(escalation: &GateWaitingPayload, shown: &[ShownDocument]) -> Section {
    let before = crate::shown::before_you_decide(escalation.withheld(), shown);
    let mut blocks: Vec<Block> = record(escalation, shown)
        .into_iter()
        .map(|line| Note(line).into())
        .collect();
    if let Some(before) = before {
        blocks.push(before.into());
    }
    let options = crate::offers::decision_options(escalation);
    blocks.push(crate::blocks::decision::at_the_prompt(options).into());
    decision(
        crate::claim::sentence(escalation.summary(), escalation.failure()),
        blocks,
    )
}
