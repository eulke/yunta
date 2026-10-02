//! A document an escalation shows, as the person deciding reads it.
//!
//! What they decide on is what the run holds, so the words come from the
//! document itself, cut to the width they read at, and the path under
//! them is where its file sits, in full. A plan is read the way it is
//! reviewed: what it changes and why, the shapes it creates, what it
//! risks and leaves out, the order its tasks run in, then task by task
//! what each does, what it touches and what proves it done — in words;
//! the commands that prove it stay in the whole plan, one open away. A
//! diagram has no room on a terminal either, so it is named here and
//! drawn there. A spec is read task by task, with its tests' files
//! whole; what a review found, the most severe first; the run's findings
//! the same way, each with the node that found it and how others
//! answered it.

use yunta_core::events::{ArtifactId, Withheld};
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::text::{agreeing, counted};

use crate::blocks::{Marked, Section};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
pub use crate::plan::{Form, CODE_SHOWN};
use crate::Mark;

/// What `document` says, for the run whose handle is `run`, in `form`.
pub fn document(document: &ShownDocument, run: &str, form: Form) -> Doc<'static> {
    let of = document
        .shown
        .producer
        .as_ref()
        .map(|node| format!(" of `{node}`"))
        .unwrap_or_default();
    match &document.content {
        ShownContent::Tasks(review) => crate::plan::document(review, &of, run, form),
        ShownContent::Spec(file) => crate::spec::document(file, &of, run, form),
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
                        .push(Tone::Muted, format!(" — {named}")),
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
/// one handover; what of it shows no code, or is checked by nothing but
/// the suite; and the risks it names. `None` when there is nothing to say.
pub fn before_you_decide(
    withheld: &[Withheld],
    shown: &[ShownDocument],
) -> Option<Section<'static>> {
    let mut blocks: Vec<Block<'static>> = Vec::new();
    for review in shown.iter().filter_map(|document| match &document.content {
        ShownContent::Tasks(review) => Some(&**review),
        _ => None,
    }) {
        blocks.extend(unprovable(withheld, review));
        let noted = noted(review);
        if !noted.is_empty() {
            blocks.push(
                Marked {
                    mark: Mark::Pending,
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
        mark: None,
        title: Line::new().push(Tone::Strong, "before you decide"),
        blocks,
    })
}

/// What keeps `review` from being proven, under what the gate withholds
/// because of it.
fn unprovable(withheld: &[Withheld], review: &PlanReview) -> Vec<Block<'static>> {
    let flaws = review.flaws();
    if flaws.is_empty() {
        return Vec::new();
    }
    let heading = match withheld {
        [] => "the plan cannot be proven as it is written".to_string(),
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
        mark: None,
        title: Line::new().push(Tone::Caution, heading),
        blocks: crate::plan::concerns(&flaws),
    }
    .into()]
}

/// What a person deciding on `review` should know and the plan does not
/// say about itself: how its planner got it accepted, and what of it
/// nothing shows or checks.
fn noted(review: &PlanReview) -> Vec<String> {
    [handed(review), codeless(review), unguarded(review)]
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
        .map(|(code, times)| format!("{code} ×{times}"))
        .collect();
    Some(format!(
        "handed over {}; refused {}: {}",
        counted(handed.submissions, "time"),
        handed.refusals,
        broke.join(", ")
    ))
}

/// How many of the plan's changes show no code: neither their own nor
/// the shape their task declares in that file.
fn codeless(review: &PlanReview) -> Option<String> {
    let plan = &review.plan;
    let changes: Vec<bool> = plan
        .tasks
        .iter()
        .flat_map(|task| {
            task.changes.iter().map(move |change| {
                change.code.is_some()
                    || plan
                        .shapes
                        .iter()
                        .any(|shape| shape.owner == task.id && shape.file == change.file())
            })
        })
        .collect();
    let codeless = changes.iter().filter(|coded| !**coded).count();
    (codeless > 0).then(|| {
        format!(
            "{codeless} of {} {} no code",
            counted(changes.len(), "change"),
            agreeing(codeless, "shows", "show")
        )
    })
}

/// How many promises the plan's tasks keep that no guard of theirs
/// checks, only the suite.
fn unguarded(review: &PlanReview) -> Option<String> {
    let unguarded: usize = review
        .plan
        .tasks
        .iter()
        .filter(|task| {
            review
                .tasks
                .iter()
                .find(|judged| judged.task == task.id)
                .is_none_or(|judged| judged.guards.is_empty())
        })
        .map(|task| task.invariants.len())
        .sum();
    (unguarded > 0).then(|| {
        format!(
            "nothing but the suite checks {} the tasks keep",
            counted(unguarded, "promise")
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
