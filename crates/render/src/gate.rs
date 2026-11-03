//! The page a gate's decision is shown on: one headline, where the decision
//! waits, what the page leaves out, what the run changed, each document in
//! the form the page shows it, and the decision a person answers last. A
//! terminal draws it above the prompt that answers the decision, and a forge
//! draws it as the description of the pull request a reviewer answers on.

use std::collections::BTreeMap;

use surco::components::{Fields, Gist, Headline, Lead, Note, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::{Glyphs, Mark};
use surco::line::{Line, Tone};
use surco::words::Standing;
use yunta_core::events::{GateWaitingPayload, Leads};
use yunta_core::shown::ShownDocument;
use yunta_core::step::NextStep;
use yunta_core::{LeftOut, NodeId, OptionId, RunId, Workflow};

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
pub fn page(page: GatePage<'_>, spell: &dyn Fn(&NextStep) -> String) -> Doc {
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
            let drawn = crate::shown::document(document, run, page.form, spell);
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

/// What follows an answer past what its label names, by rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// The run goes on past the gate and the nodes the label names: what runs after them.
    On(Vec<Vec<NodeId>>),
    /// The run goes back to the node the label names: what runs after it, the gate
    /// that asks again last.
    Back(Vec<Vec<NodeId>>),
}

impl Route {
    /// The route as a line: each rank apart by the arrow, what runs side by side
    /// apart by a comma, and a node run once per agent as its name and how many
    /// times. Going back ends on the mark of what needs a person.
    pub fn said(&self, glyphs: Glyphs) -> String {
        let (ranks, back) = match self {
            Route::On(ranks) => (ranks, false),
            Route::Back(ranks) => (ranks, true),
        };
        let last = ranks.len().saturating_sub(1);
        let said: Vec<String> = ranks
            .iter()
            .enumerate()
            .map(|(at, rank)| match (back, at == last) {
                (true, true) => {
                    format!(
                        "{} {}",
                        glyphs.mark(Mark::NeedsYou),
                        rank_said(rank, glyphs)
                    )
                }
                _ => rank_said(rank, glyphs),
            })
            .collect();
        said.join(format!(" {} ", glyphs.arrow()).as_str())
    }
}

/// One rank: each name once, in order, with how many times it runs when more than
/// once, a node run per agent being `name@agent`.
fn rank_said(rank: &[NodeId], glyphs: Glyphs) -> String {
    let mut names: Vec<(&str, usize)> = Vec::new();
    for id in rank {
        // A node run per agent is `name@agent`, and a reader counts it by its name.
        let name = id
            .as_str()
            .split_once('@')
            .map_or(id.as_str(), |(name, _)| name);
        match names.iter().position(|(seen, _)| *seen == name) {
            Some(at) => names[at].1 += 1,
            None => names.push((name, 1)),
        }
    }
    names
        .into_iter()
        .map(|(name, count)| match count {
            1 => name.to_string(),
            _ => format!("{name} {}{count}", glyphs.times()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What follows each answer `escalation` offers that leads somewhere, past what its
/// label names: going on says what runs after the nodes it goes on to, and going back
/// says what runs again before `gate` asks again. A pause leads nowhere; neither does
/// a gate nothing waits on, nor going back from a decision on no node. An answer whose
/// label and words say all of it, going on to the last node or back to the one right
/// before the gate, has no route.
pub fn routes(
    escalation: &GateWaitingPayload,
    workflow: &Workflow,
    left_out: &[LeftOut],
    gate: Option<&NodeId>,
) -> BTreeMap<OptionId, Route> {
    escalation
        .options()
        .iter()
        .filter_map(|option| {
            let route = match option.leads.as_ref()? {
                Leads::On { next, .. } => {
                    let ranks = past_the_first(workflow.ranks_from(next, left_out));
                    (!ranks.is_empty()).then_some(Route::On(ranks))?
                }
                Leads::Back { node, .. } => {
                    let ranks = past_the_first(workflow.ranks_between(node, gate?, left_out));
                    (ranks.len() > 1).then_some(Route::Back(ranks))?
                }
            };
            Some((option.id.clone(), route))
        })
        .collect()
}

/// `ranks` past the first, which is what an option's label names.
fn past_the_first(ranks: Vec<Vec<NodeId>>) -> Vec<Vec<NodeId>> {
    ranks.into_iter().skip(1).collect()
}

/// What a prompt's card stands under: each document the decision is about, in the
/// form a terminal shows it, under its headline, and what weighs on the decision.
/// `None` where the card says it all.
pub fn above(
    shown: Option<(&RunId, &[ShownDocument])>,
    escalation: &GateWaitingPayload,
    spell: &dyn Fn(&NextStep) -> String,
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
            let drawn = crate::shown::document(document, run, Form::Review, spell);
            doc = doc.with_all(drawn.into_blocks());
        }
    }
    if let Some(before) = before {
        doc = doc.with(before);
    }
    Some(doc)
}
