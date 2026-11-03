//! What proves a plan, a step away from its change: the tests the spec
//! wrote, counted, with the command that shows them whole where a review
//! leaves them out; the suite every task keeps passing, once; each guard
//! that checks what a task keeps, and the promises no guard checks; and,
//! where no decision is put beside the plan, what keeps it from being
//! proven.

use surco::components::{Fields, Fold, Marked, Section};
use surco::doc::{Block, Doc};
use surco::glyphs::Mark;
use surco::line::Line;
use surco::text::counted;
use yunta_core::shown::PlanReview;
use yunta_core::step::NextStep;
use yunta_core::RunId;

use super::{titled, Form};

/// `doc` with what proves `review`, for the run `run`; each command it
/// names is written by `spell`. What keeps a plan from being proven is
/// said here in its whole form: where a decision is made on it, it is
/// said beside the decision, once.
pub(super) fn proof(
    doc: Doc,
    review: &PlanReview,
    form: Form,
    said: (&RunId, &dyn Fn(&NextStep) -> String),
) -> Doc {
    let fields = guards(measured(review, form, said), review);
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
    if form == Form::Whole {
        blocks.extend(super::concerns(&review.flaws()));
    }
    match blocks.is_empty() {
        true => doc,
        false => doc.with(titled("proof", folded(blocks, review, form))),
    }
}

/// What holds every task: the tests the spec wrote, counted, with the
/// command that shows them whole where a review leaves them out; and the
/// suite every task keeps passing.
fn measured(
    review: &PlanReview,
    form: Form,
    (run, spell): (&RunId, &dyn Fn(&NextStep) -> String),
) -> Fields {
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
        if let (Form::Review, Some(node)) = (form, &review.spec_node) {
            fields = fields.push_command("", crate::spec::status_command(run, node, spell));
        }
    }
    if !review.held.is_empty() {
        fields = review.held.iter().fold(
            fields.push_if(
                "the suite",
                "every task keeps it passing; green before this run changed anything",
            ),
            |fields, command| fields.push_command("", command.as_str()),
        );
    }
    fields
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
        fields = match guard.proves.as_deref().map(str::trim) {
            Some(proves) if !proves.is_empty() => fields
                .push_if(label, format!("`{task}`: {proves}"))
                .push_command("", guard.cmd.as_str()),
            _ => fields.push_command(label, guard.cmd.as_str()),
        };
    }
    fields
}

/// How many promises the plan's tasks keep that no guard of theirs
/// checks: only the suite, when the run measured one.
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
    let checker = match review.held.is_empty() {
        true => "no guard of theirs",
        false => "nothing but the suite",
    };
    (unguarded > 0).then(|| {
        format!(
            "{checker} checks {} the tasks keep",
            counted(unguarded, "promise")
        )
    })
}
