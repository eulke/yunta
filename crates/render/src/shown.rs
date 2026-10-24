//! A document an escalation shows, as the person deciding reads it.
//!
//! What they decide on is what the run holds, so the words come from the
//! document itself, cut to the width they read at, and the path under
//! them is where its file sits, in full. A plan is read as the change it
//! makes, its proof a step away; a spec task by task, its tests whole and
//! what only sets them up folded; what a review found, the most severe
//! first; the run's findings the same way, each with the node that found
//! it and how others answered it. What weighs on the decision is said
//! last, beside its menu.

use surco::components::{Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::{Line, Tone};
use surco::text::{agreeing, counted};
use yunta_core::events::{ArtifactId, Withheld};
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::step::NextStep;
use yunta_core::RunId;

pub use crate::plan::Form;

/// What `document` says, for the run `run`, in `form`. Each command it
/// names is written by `spell`.
pub fn document(
    document: &ShownDocument,
    run: &RunId,
    form: Form,
    spell: &dyn Fn(&NextStep) -> String,
) -> Doc {
    let producer = document.shown.producer.as_ref();
    let of = producer
        .map(|node| format!(" of `{node}`"))
        .unwrap_or_default();
    match &document.content {
        ShownContent::Tasks(review) => crate::plan::document(review, run, form, spell),
        ShownContent::Spec(file) => crate::spec::document(file, (&of, producer), run, form, spell),
        ShownContent::Findings(file) => crate::findings::document(file, &of),
        ShownContent::RunFindings(view) => crate::findings::run_document(view),
        ShownContent::Text(text) => {
            let named = match &document.shown.artifact {
                ArtifactId::Interpreted { kind } => format!("the {kind} document{of}"),
                ArtifactId::Opaque { name } => format!("{name}{of}"),
            };
            // A document that opens on a heading is called what it calls
            // itself — what a brief asks for, before which file it is.
            let (title, body) = match headed(text) {
                Some((heading, body)) => (
                    Line::new()
                        .push(Tone::Strong, heading)
                        .push(Tone::Muted, format!(": {named}")),
                    body,
                ),
                None => (Line::new().push(Tone::Strong, named), text.as_str()),
            };
            Doc::new()
                .with(Block::Title(title))
                .with(Block::Markdown(body.to_string()))
        }
    }
}

/// The heading `text` opens on, and what follows it; `None` when it
/// opens on anything else.
fn headed(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let heading = first.trim_start_matches('#');
    let marks = first.len() - heading.len();
    let heading = heading.strip_prefix(' ')?.trim();
    ((1..=6).contains(&marks) && !heading.is_empty()).then_some((heading, rest))
}

/// What weighs on a decision, said beside its menu: on a terminal the
/// last lines printed are the first ones read, so what a person should
/// know before choosing sits between the question and the options.
///
/// What the gate does not offer, and each flaw that keeps the plan from
/// being proven; how its planner got it accepted, when it took more than
/// one handover; what of it shows no code; and the risks it names. `None`
/// when there is nothing to say.
pub fn before_you_decide(withheld: &[Withheld], shown: &[ShownDocument]) -> Option<Section> {
    let mut blocks: Vec<Block> = Vec::new();
    for review in shown.iter().filter_map(|document| match &document.content {
        ShownContent::Tasks(review) => Some(&**review),
        _ => None,
    }) {
        blocks.extend(unprovable(withheld, review));
        let noted = noted(review);
        if !noted.is_empty() {
            blocks.push(
                Marked {
                    mark: Mark::Absent,
                    items: noted,
                }
                .into(),
            );
        }
        let risks: Vec<String> = review
            .plan
            .risks
            .iter()
            .map(|risk| format!("risk: {}", first_sentence(risk)))
            .collect();
        if !risks.is_empty() {
            blocks.push(
                Marked {
                    mark: Mark::Caution,
                    items: risks,
                }
                .into(),
            );
        }
    }
    (!blocks.is_empty()).then(|| Section {
        lead: None,
        title: Line::new().push(Tone::Strong, "before you decide"),
        blocks,
    })
}

/// What keeps `review` from being proven, under what the gate withholds
/// because of it.
fn unprovable(withheld: &[Withheld], review: &PlanReview) -> Vec<Block> {
    let flaws = review.flaws();
    if flaws.is_empty() {
        return Vec::new();
    }
    let heading = match withheld {
        [] => yunta_core::shown::UNPROVABLE_PLAN.to_string(),
        _ => {
            let options: Vec<String> = withheld
                .iter()
                .map(|gone| format!("`{}`", gone.option))
                .collect();
            format!(
                "{} {} not offered, because",
                options.join(", "),
                agreeing(options.len(), "is", "are"),
            )
        }
    };
    vec![Section {
        lead: None,
        title: Line::new().push(Tone::Caution, heading),
        blocks: crate::plan::concerns(&flaws),
    }
    .into()]
}

/// What a person deciding on `review` should know and the plan does not
/// say about itself: how its planner got it accepted, and what of it
/// shows no code.
fn noted(review: &PlanReview) -> Vec<String> {
    [handed(review), codeless(review)]
        .into_iter()
        .flatten()
        .collect()
}

/// How the planner got `review` accepted, when it took more than one
/// handover.
fn handed(review: &PlanReview) -> Option<String> {
    let handed = review
        .handed_over
        .as_ref()
        .filter(|handed| handed.refusals > 0)?;
    let broke: Vec<String> = handed
        .refused
        .iter()
        .map(|(code, times)| format!("{code} {}", counted(*times, "time")))
        .collect();
    Some(format!(
        "handed over {}; refused {}: {}",
        counted(handed.submissions, "time"),
        handed.refusals,
        broke.join(", ")
    ))
}

/// How many of the changes the plan's cards draw show no code: neither
/// their own nor a shape of their task's.
fn codeless(review: &PlanReview) -> Option<String> {
    let (codeless, changes) = crate::plan::codeless(review);
    (codeless > 0).then(|| {
        format!(
            "{codeless} of {} {} no code",
            counted(changes, "change"),
            agreeing(codeless, "shows", "show")
        )
    })
}

/// The first sentence of `text`, for a list that names each thing once.
fn first_sentence(text: &str) -> &str {
    let text = text.trim();
    match text.find(". ") {
        Some(end) => &text[..=end],
        None => text,
    }
}
