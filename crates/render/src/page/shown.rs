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
use surco::look::Look;
use surco::text::{agreeing, counted};
use yunta_core::events::{ArtifactId, Withheld};
use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
use yunta_core::RunId;

use super::{Findings, Form, Page, Plan, Spec};

/// A document an escalation shows, as the page the person deciding reads:
/// `document`, for the run `run`, in `form`.
#[derive(Debug, Clone, Copy)]
pub struct Shown<'a> {
    pub document: &'a ShownDocument,
    pub run: &'a RunId,
    pub form: Form,
}

/// What the document says, as the page of what it is: a plan, a spec or
/// findings drawn as theirs, with this page's look, and a text under the
/// title it is called by.
impl Page for Shown<'_> {
    fn document(&self, look: &Look) -> Doc {
        let Shown {
            document,
            run,
            form,
        } = *self;
        let producer = document.shown.producer.as_ref();
        let of = producer
            .map(|node| format!(" of `{node}`"))
            .unwrap_or_default();
        match &document.content {
            ShownContent::Tasks(review) => Plan { review, run, form }.document(look),
            ShownContent::Spec(file) => Spec {
                file,
                of: &of,
                producer,
                run,
                form,
            }
            .document(look),
            ShownContent::Findings(held) => Findings::Held { held, of: &of }.document(look),
            ShownContent::RunFindings(view) => Findings::Run(view).document(look),
            ShownContent::Text(text) => text_document(document, text, &of),
        }
    }
}

/// A text document, `of` the node that produced it: titled by the heading it
/// opens on, else by its artifact, and its words after.
fn text_document(document: &ShownDocument, text: &str, of: &str) -> Doc {
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
        None => (Line::new().push(Tone::Strong, named), text),
    };
    let doc = Doc::new().with(Block::Title(surco::components::Title::named(title)));
    match body.trim().is_empty() {
        true => doc,
        false => doc.with(Block::Markdown(body.to_string())),
    }
}

/// What a document is called on a page that shows it beside others: the
/// heading a text document opens on, else its artifact, named as its page
/// names it without the producer it came from.
pub(crate) fn called(document: &ShownDocument) -> String {
    let heading = match &document.content {
        ShownContent::Text(text) => headed(text).map(|(heading, _)| heading.to_string()),
        _ => None,
    };
    heading.unwrap_or_else(|| match &document.shown.artifact {
        ArtifactId::Interpreted { kind } => kind.to_string(),
        ArtifactId::Opaque { name } => name.to_string(),
    })
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

/// The line a text document's heading is written on; empty when it opens on none.
pub(crate) fn heading_line(text: &str) -> String {
    match headed(text) {
        Some(_) => text
            .trim_start()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
        None => String::new(),
    }
}

/// What weighs on a decision, said beside its menu: on a terminal the
/// last lines printed are the first ones read, so what a person should
/// know before choosing sits between the question and the options.
///
/// What the gate does not offer, and each flaw that keeps the plan from
/// being proven; how its planner got it accepted, when it took more than
/// one handover; what of it shows no code; and the risks it names. `None`
/// when there is nothing to say.
pub(crate) fn before_you_decide(withheld: &[Withheld], shown: &[ShownDocument]) -> Option<Section> {
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
            .map(|risk| format!("risk: {risk}"))
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

/// Whether the section `before_you_decide` draws over `shown` says why the
/// options `withheld` names are not offered: it does when a plan among
/// `shown` has a flaw, under which `unprovable` names each withheld option.
pub(crate) fn says_why(withheld: &[Withheld], shown: &[ShownDocument]) -> bool {
    !withheld.is_empty()
        && shown.iter().any(|document| match &document.content {
            ShownContent::Tasks(review) => !review.flaws().is_empty(),
            _ => false,
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
        blocks: super::plan::concerns(&flaws),
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
        // Each rule counts the refusals that broke it, and one refusal can break two.
        .map(|(code, times)| format!("{code} in {times}"))
        .collect();
    Some(format!(
        "handed over {}; refused {}: {}",
        counted(handed.submissions, "time"),
        counted(handed.refusals, "time"),
        broke.join(", ")
    ))
}

/// How many of the changes the plan's cards draw show no code: neither
/// their own nor a shape of their task's.
fn codeless(review: &PlanReview) -> Option<String> {
    let (codeless, changes) = super::plan::codeless(review);
    (codeless > 0).then(|| {
        format!(
            "{codeless} of {} {} no code",
            counted(changes, "change"),
            agreeing(codeless, "shows", "show")
        )
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use surco::components::Section;
    use surco::doc::Block;
    use surco::glyphs::Mark;
    use yunta_core::events::{ArtifactId, Shown};
    use yunta_core::shown::{PlanReview, ShownContent, ShownDocument};
    use yunta_core::{ArtifactKind, ContentHash, TasksFile};

    use super::before_you_decide;

    /// A plan document that names `risks` and nothing else: no task and no
    /// spec, so the only thing a decision weighs on it is what it names.
    fn plan_naming(risks: &[&str]) -> ShownDocument {
        let plan = TasksFile {
            summary: None,
            description: None,
            design: None,
            decisions: Vec::new(),
            shapes: Vec::new(),
            risks: risks.iter().map(|risk| risk.to_string()).collect(),
            out_of_scope: Vec::new(),
            tasks: Vec::new(),
        };
        ShownDocument {
            shown: Shown {
                producer: Some("plan".into()),
                artifact: ArtifactId::Interpreted {
                    kind: ArtifactKind::Tasks,
                },
                content_hash: ContentHash::sha256(b"plan"),
            },
            path: PathBuf::from("artifacts/plan/tasks.yaml"),
            content: ShownContent::Tasks(Box::new(PlanReview {
                plan,
                departed: Vec::new(),
                spec: None,
                spec_node: None,
                asked: None,
                held: Vec::new(),
                tasks: Vec::new(),
                handed_over: None,
                answered: Vec::new(),
                shapes_read: None,
            })),
        }
    }

    /// The items of the caution marks `section` holds, in the order they are
    /// drawn.
    fn cautions(section: &Section) -> Vec<String> {
        section
            .blocks
            .iter()
            .flat_map(|block| match block {
                Block::Marked(marked) if matches!(marked.mark, Mark::Caution) => {
                    marked.items.clone()
                }
                _ => Vec::new(),
            })
            .collect()
    }

    /// A risk of two sentences is said whole as one item, and so is each risk
    /// after it.
    #[test]
    fn each_risk_is_said_whole_as_its_own_item() {
        let shown = [plan_naming(&[
            "Two tasks write one file. Their order decides what it holds.",
            "The spec's test is slow.",
        ])];
        let section = before_you_decide(&[], &shown).expect("a named risk weighs on the decision");
        assert_eq!(
            cautions(&section),
            vec![
                "risk: Two tasks write one file. Their order decides what it holds.",
                "risk: The spec's test is slow.",
            ]
        );
    }
}
