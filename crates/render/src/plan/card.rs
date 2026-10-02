//! One task's card: what a person sees once it is done, what it touches,
//! keeps and waits on; each change it makes with the code the plan writes
//! for it; what proves it done, with the code of the test that does.

use yunta_core::shown::{HeldTo, JudgedCriterion, PlanReview, TaskReview};
use yunta_core::{Task, TasksFile};

use super::{blank, grouped, said, Form, CODE_SHOWN};
use crate::blocks::{Code, Fields, Marked, Section};
use crate::doc::Block;
use crate::ink::{Line, Tone};
use crate::Mark;

/// One task: what a person sees once it is done, what it touches, keeps
/// and waits on; then each change it makes with the code the plan writes
/// for it; then what proves it done, with the code of the test.
pub(super) fn card(task: &Task, review: &PlanReview, form: Form, run: &str) -> Section<'static> {
    let judged = review.tasks.iter().find(|judged| judged.task == task.id);
    let mut blocks: Vec<Block<'static>> = vec![facts(task, &review.plan).into()];
    for change in changes(task, &review.plan, judged) {
        blocks.push(blank());
        blocks.push(change);
    }
    if let Some(judged) = judged {
        for criterion in &judged.criteria {
            blocks.push(blank());
            blocks.extend(proved(criterion, judged, review, form, run));
        }
    }
    Section {
        mark: None,
        title: Line::new()
            .push(Tone::Strong, task.id.as_str())
            .plain(format!(" — {}", task.title)),
        blocks,
    }
}

/// What a task's card says in words.
fn facts(task: &Task, plan: &TasksFile) -> Fields {
    let mut fields = Fields::new();
    if let Some(outcome) = said(&task.outcome) {
        fields = fields.push_if("you will see", outcome);
    }
    fields = fields.push_if("touches", grouped(&task.scope).join(", "));
    if !task.uses.is_empty() {
        let uses: Vec<String> = task
            .uses
            .iter()
            .map(
                |name| match plan.shapes.iter().find(|shape| shape.name == *name) {
                    Some(shape) => format!("{name} (from {})", shape.owner),
                    None => name.clone(),
                },
            )
            .collect();
        fields = fields.push_if("uses", uses.join(", "));
    }
    for (n, invariant) in task.invariants.iter().enumerate() {
        fields = fields.push_if(if n == 0 { "keeps" } else { "" }, invariant.as_str());
    }
    if !task.depends_on.is_empty() {
        let after: Vec<&str> = task.depends_on.iter().map(|id| id.as_str()).collect();
        fields = fields.push_if("after", after.join(", "));
    }
    fields
}

/// Each change a task makes: where, what for, and the code the plan
/// writes for it — or, for a test the spec wrote, that no session may.
fn changes(task: &Task, plan: &TasksFile, judged: Option<&TaskReview>) -> Vec<Block<'static>> {
    task.changes
        .iter()
        .map(|change| {
            let at = change.at.replace("::", " › ");
            let denied = judged
                .into_iter()
                .flat_map(|judged| &judged.denied)
                .find(|denied| denied.at == change.at);
            if let Some(denied) = denied {
                return Marked {
                    mark: Mark::Caution,
                    items: vec![format!(
                        "{at} — a test the spec wrote for `{}`; no session may write it",
                        denied.owner
                    )],
                }
                .into();
            }
            let shape = plan
                .shapes
                .iter()
                .find(|shape| shape.owner == task.id && shape.file == change.file());
            match shape {
                Some(shape) => Code::whole(at, Some(change.what.clone()), &shape.code).into(),
                None => Code {
                    at,
                    what: Some(change.what.clone()),
                    lines: Vec::new(),
                    whole: 0,
                    rest: None,
                }
                .into(),
            }
        })
        .collect()
}

/// One criterion: what it proves, the command that runs it, and the code
/// of each file the spec wrote that the command runs.
fn proved(
    criterion: &JudgedCriterion,
    judged: &TaskReview,
    review: &PlanReview,
    form: Form,
    run: &str,
) -> Vec<Block<'static>> {
    let mut fields = Fields::new();
    if let Some(proves) = criterion.proves.as_deref() {
        fields = fields.push_if("done when", proves);
    }
    fields = fields.push_command(
        if criterion.proves.is_some() {
            ""
        } else {
            "done when"
        },
        format!("$ {}", criterion.cmd),
    );
    let mut blocks: Vec<Block<'static>> = Vec::new();
    match &criterion.from {
        HeldTo::Spec => {
            blocks.push(fields.into());
            blocks.extend(tests_run(criterion, judged, review, form, run));
        }
        HeldTo::AnotherTask { task } => {
            blocks.push(
                fields
                    .push_if("", format!("the same command as the spec test of `{task}`"))
                    .into(),
            );
        }
        HeldTo::Plan => blocks.push(fields.into()),
    }
    blocks
}

/// The code of each file the spec wrote that `criterion` runs.
fn tests_run(
    criterion: &JudgedCriterion,
    judged: &TaskReview,
    review: &PlanReview,
    form: Form,
    run: &str,
) -> Vec<Block<'static>> {
    judged
        .files
        .iter()
        .filter(|file| file.run_by.contains(&criterion.cmd))
        .map(|file| {
            let content = review
                .spec
                .iter()
                .flat_map(|spec| &spec.specs)
                .flat_map(|spec| &spec.files)
                .find(|test| test.path == file.path)
                .map(|test| test.content.as_str())
                .unwrap_or_default();
            let code = Code::whole(
                file.path.as_str(),
                Some("the test the spec wrote for this task".to_string()),
                content,
            );
            match form {
                Form::Review => code.cut(CODE_SHOWN, format!("yunta status {run} --node spec")),
                Form::Whole => code,
            }
            .into()
        })
        .collect()
}
