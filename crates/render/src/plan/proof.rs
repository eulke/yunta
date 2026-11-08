//! What proves a plan, a step away from its change: the tests the spec
//! wrote, counted, with the command that shows them whole where a review
//! leaves them out; the suite every task keeps passing, once; each guard
//! that checks what a task keeps, and the promises no guard checks; and,
//! where no decision is put beside the plan, what keeps it from being
//! proven.

use surco::components::{Code, Fields, Fold, Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::Line;
use surco::text::counted;
use yunta_core::shown::{JudgedCriterion, PlanReview};
use yunta_core::{RunId, Task, TaskId};

use super::{card, titled, Form};

/// `doc` with what proves `review`, for the run `run`, the one each command
/// it names takes. What keeps a plan from being proven is
/// said here in its whole form: where a decision is made on it, it is
/// said beside the decision, once.
pub(super) fn proof(doc: Doc, review: &PlanReview, form: Form, run: &RunId) -> Doc {
    let showing = match (form, &review.spec_node) {
        (Form::Review, Some(node)) => Some(crate::spec::status_command(run, node)),
        _ => None,
    };
    let mut blocks = held(review, showing);
    if form == Form::Whole {
        blocks.extend(super::concerns(&review.flaws()));
    }
    match blocks.is_empty() {
        true => doc,
        false => doc.with(titled("proof", folded(blocks, review, form))),
    }
}

/// What proves the plan as a whole, on the desk, where each file the spec
/// wrote is a key away: the tests counted, the suite, every guard and the
/// promises no guard checks.
pub(super) fn of_plan(review: &PlanReview) -> Doc {
    let mut blocks = held(review, None);
    if blocks.is_empty() {
        blocks.push(nothing("nothing here proves the plan"));
    }
    Doc::new().with_all(blocks)
}

/// What proves `task`, a key away from its card: what tests it, the suite
/// every task keeps passing, its guards, what it promises that no guard of
/// its own checks, and each file the spec wrote for it, whole.
pub(super) fn of_task(task: &Task, review: &PlanReview) -> Doc {
    let judged = card::judged(review, task);
    let fields = suite(card::tested(Fields::new(), task, review, judged), review);
    let guards = judged.map_or(&[][..], |judged| judged.guards.as_slice());
    let fields = guards
        .iter()
        .enumerate()
        .fold(fields, |fields, (n, guard)| {
            guarded(fields, if n == 0 { "guards" } else { "" }, None, guard)
        });
    let mut blocks: Vec<Block> = Vec::new();
    if !fields.is_empty() {
        blocks.push(fields.into());
    }
    if !task.invariants.is_empty() && guards.is_empty() {
        blocks.push(
            Marked {
                mark: Mark::Absent,
                items: vec![format!(
                    "{} checks {} this task keeps",
                    unguarded_by(review),
                    counted(task.invariants.len(), "promise")
                )],
            }
            .into(),
        );
    }
    if let Some(spec) = review.spec.as_ref().and_then(|spec| spec.of(&task.id)) {
        blocks.extend(spec.files.iter().map(|file| {
            Code::whole(
                file.path.as_str(),
                Some("written by the spec".to_string()),
                &file.content,
            )
            .into()
        }));
    }
    if blocks.is_empty() {
        blocks.push(nothing("nothing here proves this task"));
    }
    Doc::new().with_all(blocks)
}

/// The blocks that hold every task: the fields of what the spec tests and
/// the suite, each guard a task keeps, and the promises no guard checks.
/// `showing` is the command that shows the spec's tests whole, when a
/// review leaves them to it.
pub(super) fn held(review: &PlanReview, showing: Option<String>) -> Vec<Block> {
    let fields = guards(measured(review, showing), review);
    let mut blocks: Vec<Block> = Vec::new();
    if !fields.is_empty() {
        blocks.push(fields.into());
    }
    if let Some(unguarded) = unguarded(review) {
        blocks.push(
            Marked {
                mark: Mark::Absent,
                items: vec![unguarded],
            }
            .into(),
        );
    }
    blocks
}

/// A block that says nothing is there to prove what it is about, so a pane
/// is not drawn empty.
fn nothing(said: &str) -> Block {
    Marked {
        mark: Mark::Absent,
        items: vec![said.to_string()],
    }
    .into()
}

/// What the spec tests, counted, with `showing` the command that shows them
/// whole; and the suite every task keeps passing.
fn measured(review: &PlanReview, showing: Option<String>) -> Fields {
    let mut fields = Fields::new();
    if let Some(spec) = &review.spec {
        let tests = crate::spec::test_count(spec);
        let files: Vec<&str> = spec
            .specs
            .iter()
            .flat_map(|spec| &spec.files)
            .map(|file| file.content.as_str())
            .collect();
        let lines: usize = files
            .iter()
            .map(|content| content.trim_end().lines().count())
            .sum();
        fields = fields.push_if(
            "spec tests",
            format!(
                "{tests} in {}, {}",
                counted(files.len(), "file"),
                counted(lines, "line")
            ),
        );
        if let Some(showing) = showing {
            fields = fields.push_command("", showing);
        }
    }
    suite(fields, review)
}

/// `fields` with the suite every task keeps passing, and the command that
/// runs it, once for all of them. Nothing when the run measured no suite.
pub(super) fn suite(fields: Fields, review: &PlanReview) -> Fields {
    match review.held.is_empty() {
        true => fields,
        false => review.held.iter().fold(
            fields.push_if(
                "the suite",
                "every task keeps it passing; green before this run changed anything",
            ),
            |fields, command| fields.push_command("", command.as_str()),
        ),
    }
}

/// `blocks` as the proof draws them: a forge's page folds them under what
/// sums them up, when anything does; every other form draws them open.
fn folded(blocks: Vec<Block>, review: &PlanReview, form: Form) -> Vec<Block> {
    match form {
        Form::Published => match summary(review) {
            Some(words) => vec![Fold(Section {
                lead: None,
                title: Line::new().plain(words),
                blocks,
            })
            .into()],
            None => blocks,
        },
        Form::Review | Form::Browser | Form::Whole => blocks,
    }
}

/// What a folded proof is summed up by: what holds the plan, counted:
/// `4 spec tests and the suite`, `2 guards and the suite`, `the suite`.
/// `None` when none of them holds it, and the proof is not folded.
fn summary(review: &PlanReview) -> Option<String> {
    let tests = review.spec.as_ref().map_or(0, crate::spec::test_count);
    let guards: usize = review.tasks.iter().map(|judged| judged.guards.len()).sum();
    let parts: Vec<String> = [
        (tests > 0).then(|| counted(tests, "spec test")),
        (guards > 0).then(|| counted(guards, "guard")),
        (!review.held.is_empty()).then(|| "the suite".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect();
    match parts.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        [init @ .., last] => Some(format!("{} and {last}", init.join(", "))),
    }
}

/// `fields` with each guard a task keeps a promise by: the task, what it
/// proves, and the command that runs it.
fn guards(mut fields: Fields, review: &PlanReview) -> Fields {
    let guards = review
        .tasks
        .iter()
        .flat_map(|judged| judged.guards.iter().map(move |guard| (&judged.task, guard)));
    for (n, (task, guard)) in guards.enumerate() {
        let label = if n == 0 { "guards" } else { "" };
        fields = guarded(fields, label, Some(task), guard);
    }
    fields
}

/// `fields` with `guard` under `label`: what it proves, with the task it
/// guards named when `task` is given, and the command that runs it.
fn guarded(
    fields: Fields,
    label: &'static str,
    task: Option<&TaskId>,
    guard: &JudgedCriterion,
) -> Fields {
    match guard.proves.as_deref().map(str::trim) {
        Some(proves) if !proves.is_empty() => {
            let said = match task {
                Some(task) => format!("`{task}`: {proves}"),
                None => proves.to_string(),
            };
            fields
                .push_if(label, said)
                .push_command("", guard.cmd.as_str())
        }
        _ => fields.push_command(label, guard.cmd.as_str()),
    }
}

/// How many promises the plan's tasks keep that no guard of theirs
/// checks, and what checks them.
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
            "{} checks {} the tasks keep",
            unguarded_by(review),
            counted(unguarded, "promise")
        )
    })
}

/// What checks the promises no guard of a task checks: the suite, when the
/// run measured one, and no guard of theirs otherwise.
fn unguarded_by(review: &PlanReview) -> &'static str {
    match review.held.is_empty() {
        true => "no guard of theirs",
        false => "nothing but the suite",
    }
}
