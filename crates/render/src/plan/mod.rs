//! A plan as the person deciding on it reads it: what it changes, its
//! tasks at a glance with what proves each, what of it cannot be done as
//! written, why it changes what it does, the
//! choices it makes, what it risks and leaves out, then step by step each
//! task — what a person sees once it is done, what it touches and keeps,
//! the code of each change it makes and what proves it done, with the
//! code of the test that does.
//!
//! The code sits in the task it belongs to: understanding a task is
//! seeing what it will write, and a shape drawn under the design or a
//! test drawn in a second document after every task is a screen away
//! from the task it explains.

use yunta_core::events::AcceptedDeparture;
use yunta_core::shown::PlanReview;
use yunta_core::text::counted;
use yunta_core::{Decision, ScopeGlob, Task, TasksFile};

use crate::blocks::{Fields, Marked, Next, Prose, Section};
use crate::doc::{Block, Doc};
use crate::ink::{Line, Tone};
use crate::Mark;

mod card;
mod map;

use card::card;

/// How much of a plan a reader is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Where a decision is made on it: every section, a description's
    /// first paragraph, and no file longer than [`CODE_SHOWN`] lines.
    Review,
    /// Everything the plan says, every file whole.
    Whole,
}

/// The lines of a file a task's card shows before the rest is a command
/// away: the length of a test that checks one behavior, so a card shows
/// what the task's tests do without a file pushing every other task off
/// the screen.
pub const CODE_SHOWN: usize = 60;

/// `review` as a document: `of` names the node that wrote the plan, and
/// `run` the handle a person reads the whole plan by.
pub fn document(review: &PlanReview, of: &str, run: &str, form: Form) -> Doc<'static> {
    let plan = &review.plan;
    let steps = plan.steps();
    let mut doc = Doc::new().with(Block::Title(
        Line::new()
            .push(Tone::Strong, format!("plan{of}"))
            .plain(format!(": {}", sized(plan, steps.len(), review))),
    ));
    if let Some(summary) = said(&plan.summary) {
        doc = doc.with(Prose(summary.to_string()));
    }
    doc = doc.with(titled("tasks", vec![map::map(review, &steps).into()]));
    doc = cannot_be_done(doc, review);
    if !review.departed.is_empty() {
        doc = doc.with(departures(&review.departed));
    }
    if let Some(description) = said(&plan.description) {
        doc = described(doc.with(blank()), description, form);
    }
    doc = designed(doc, plan, &steps);
    doc = bounded(doc, review);
    let total = steps.len();
    for (at, step) in steps.iter().enumerate() {
        let cards = step
            .iter()
            .map(|task| card(task, review, form, run).into())
            .collect();
        doc = doc.with(titled(&format!("step {} of {total}", at + 1), cards));
    }
    match form {
        Form::Review => doc.with(blank()).with(Next {
            steps: vec![(
                format!("yunta status {run} --node {}", of_node(of)),
                "the whole plan, every file whole",
            )],
        }),
        Form::Whole => doc,
    }
}

/// What the plan risks and leaves out, and the suite every task keeps
/// passing.
fn bounded(mut doc: Doc<'static>, review: &PlanReview) -> Doc<'static> {
    let plan = &review.plan;
    for (title, mark, items) in [
        ("risks", Mark::Caution, &plan.risks),
        ("out of scope", Mark::Pending, &plan.out_of_scope),
    ] {
        if !items.is_empty() {
            doc = doc.with(titled(
                title,
                vec![Marked {
                    mark,
                    items: items.clone(),
                }
                .into()],
            ));
        }
    }
    if let Some(suite) = &review.suite {
        doc = doc.with(titled(
            "every task keeps passing",
            vec![Fields::new()
                .push_if("the suite", "green before this run changed anything")
                .push_command("", format!("$ {suite}"))
                .into()],
        ));
    }
    doc
}

/// How big the plan is, in one line: its tasks and steps, and the tests
/// the spec holds them to.
fn sized(plan: &TasksFile, steps: usize, review: &PlanReview) -> String {
    let tasks = counted(plan.tasks.len(), "task");
    let mut said = match steps {
        0 | 1 => tasks,
        n => format!("{tasks} in {n} steps"),
    };
    if let Some(spec) = &review.spec {
        let tests: usize = spec.specs.iter().map(|spec| spec.tests.len()).sum();
        said.push_str(&format!(
            ", held to {} from its spec",
            counted(tests, "test")
        ));
    }
    said
}

/// What the plan says a session will do and no session may: changes on
/// a test the spec wrote, said before any detail.
fn cannot_be_done(doc: Doc<'static>, review: &PlanReview) -> Doc<'static> {
    let denied: Vec<String> = review
        .tasks
        .iter()
        .flat_map(|task| {
            task.denied
                .iter()
                .map(move |change| format!("{} — {}", task.task, change.at))
        })
        .collect();
    if denied.is_empty() {
        return doc;
    }
    let planning = review
        .tasks
        .iter()
        .filter(|task| !task.denied.is_empty())
        .count();
    doc.with(Section {
        mark: None,
        title: Line::new().push(
            Tone::Caution,
            format!(
                "{} {} to change a test the spec wrote, and no session may write one",
                counted(planning, "task"),
                if planning == 1 { "plans" } else { "plan" },
            ),
        ),
        blocks: vec![Marked {
            mark: Mark::Caution,
            items: denied,
        }
        .into()],
    })
}

/// Every departure from the plan a person accepted: what the plan says,
/// what was built instead and why.
fn departures(departed: &[AcceptedDeparture]) -> Section<'static> {
    let blocks = departed
        .iter()
        .map(|departure| {
            let declared = &departure.declared;
            let mut fields = Fields::new()
                .push_if("the plan", declared.planned.as_str())
                .push_if("built", declared.instead.as_str())
                .push_if("because", declared.why.as_str());
            if let Some(said) = said(&departure.said) {
                fields = fields.push_if("accepted", said);
            }
            Section {
                mark: None,
                title: Line::new().plain(format!(
                    "{} departs from {}",
                    declared.task_id, declared.from
                )),
                blocks: vec![fields.into()],
            }
            .into()
        })
        .collect();
    titled("accepted departures from this plan", blocks)
}

/// The description: its first paragraph where a decision is made, and
/// where a diagram is; all of it, diagrams included, otherwise.
fn described(doc: Doc<'static>, description: &str, form: Form) -> Doc<'static> {
    match form {
        Form::Whole => doc.with(Block::Markdown(description.to_string())),
        Form::Review => {
            let first = description.split("\n\n").next().unwrap_or(description);
            let mut doc = doc.with(Block::Markdown(first.to_string()));
            if description.contains("```mermaid") && !first.contains("```mermaid") {
                doc = doc.with(Fields::new().push_if("diagram", "in the whole plan"));
            }
            doc
        }
    }
}

/// What the plan decided, and how it is built: each decision, the design,
/// and where each shape's code is.
fn designed(mut doc: Doc<'static>, plan: &TasksFile, steps: &[Vec<&Task>]) -> Doc<'static> {
    if !plan.decisions.is_empty() {
        let blocks = plan
            .decisions
            .iter()
            .enumerate()
            .map(|(at, decision)| decided(at + 1, decision).into())
            .collect();
        doc = doc.with(titled("decided for you", blocks));
    }
    if said(&plan.design).is_none() && plan.shapes.is_empty() {
        return doc;
    }
    let mut blocks: Vec<Block<'static>> = Vec::new();
    if let Some(design) = said(&plan.design) {
        blocks.push(Block::Markdown(design.to_string()));
    }
    let step_of = |task: &yunta_core::TaskId| {
        steps
            .iter()
            .position(|step| step.iter().any(|candidate| candidate.id == *task))
            .map_or(0, |at| at + 1)
    };
    let shapes: Vec<Line> = plan
        .shapes
        .iter()
        .map(|shape| {
            Line::new()
                .plain("  ")
                .push(Tone::Strong, shape.name.as_str())
                .plain(format!(" in {}", shape.file))
                .push(
                    Tone::Muted,
                    format!(
                        " — its code is in step {}, `{}`",
                        step_of(&shape.owner),
                        shape.owner
                    ),
                )
        })
        .collect();
    if !shapes.is_empty() {
        blocks.push(shapes.into());
    }
    doc.with(titled("design", blocks))
}

/// One decision: what was open, what the plan chose and why, and what it
/// did not choose.
fn decided(at: usize, decision: &Decision) -> Section<'static> {
    let mut fields = Fields::new().push_if("chose", decision.choice.as_str());
    if let Some(why) = said(&decision.why) {
        fields = fields.push_if("because", why);
    }
    for (n, alternative) in decision.alternatives.iter().enumerate() {
        fields = fields.push_if(if n == 0 { "not" } else { "" }, alternative.as_str());
    }
    Section {
        mark: None,
        title: Line::new().push(Tone::Strong, format!("{at}  {}", decision.question)),
        blocks: vec![fields.into()],
    }
}

/// `scope` as a reader scans it: globs that share a directory under that
/// directory once, in the order the task names them.
pub(super) fn grouped(scope: &[ScopeGlob]) -> Vec<String> {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for glob in scope {
        let glob = glob.as_str();
        let (dir, name) = glob.rsplit_once('/').unwrap_or(("", glob));
        match groups.iter_mut().find(|(seen, _)| *seen == dir) {
            Some((_, names)) => names.push(name),
            None => groups.push((dir, vec![name])),
        }
    }
    groups
        .into_iter()
        .map(|(dir, names)| match (dir, names.as_slice()) {
            ("", _) => names.join(", "),
            (_, [one]) => format!("{dir}/{one}"),
            _ => format!("{dir}/{{{}}}", names.join(", ")),
        })
        .collect()
}

/// A blank line between two blocks a reader takes apart.
pub(super) fn blank() -> Block<'static> {
    Block::Lines(vec![Line::new()])
}

/// A section titled `title`, in the plan's own heading tone.
fn titled(title: &str, blocks: Vec<Block<'static>>) -> Section<'static> {
    Section {
        mark: None,
        title: Line::new().push(Tone::Strong, title),
        blocks,
    }
}

/// The node a plan's `of` names, for the command that shows it whole.
fn of_node(of: &str) -> &str {
    of.trim_start_matches(" of ").trim_matches('`')
}

/// Text that says something, trimmed.
pub(super) fn said(text: &Option<String>) -> Option<&str> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_s_scope_is_grouped_by_the_directory_its_globs_share() {
        let scope: Vec<ScopeGlob> = [
            "crates/cli/src/render/color.rs",
            "crates/cli/src/render/mod.rs",
            "crates/cli/src/error.rs",
            "Cargo.toml",
        ]
        .into_iter()
        .map(ScopeGlob::from)
        .collect();
        assert_eq!(
            grouped(&scope),
            vec![
                "crates/cli/src/render/{color.rs, mod.rs}",
                "crates/cli/src/error.rs",
                "Cargo.toml",
            ]
        );
    }
}
