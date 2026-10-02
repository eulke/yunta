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
    let mut blocks: Vec<Block<'static>> = vec![facts(task, &review.plan, judged).into()];
    for change in changes(task, &review.plan, judged) {
        blocks.push(blank());
        blocks.push(change);
    }
    if let Some(judged) = judged {
        for criterion in &judged.criteria {
            blocks.push(blank());
            blocks.extend(proved(criterion, judged, review, form, run));
        }
        let unrun: Vec<String> = judged
            .files
            .iter()
            .filter(|file| file.run_by.is_empty())
            .map(|file| {
                format!(
                    "{} — written by the spec; none of its tests runs it",
                    file.path
                )
            })
            .collect();
        if !unrun.is_empty() {
            blocks.push(blank());
            blocks.push(
                Marked {
                    mark: Mark::Caution,
                    items: unrun,
                }
                .into(),
            );
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

/// What a task's card says in words: what it keeps beside what checks
/// that it does.
fn facts(task: &Task, plan: &TasksFile, judged: Option<&TaskReview>) -> Fields {
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
    fields = checked(fields, task, judged);
    if !task.depends_on.is_empty() {
        let after: Vec<&str> = task.depends_on.iter().map(|id| id.as_str()).collect();
        fields = fields.push_if("after", after.join(", "));
    }
    fields
}

/// What checks that a task keeps what it promises: each of its guards,
/// or that only the suite does.
fn checked(mut fields: Fields, task: &Task, judged: Option<&TaskReview>) -> Fields {
    let guards = judged
        .map(|judged| judged.guards.as_slice())
        .unwrap_or_default();
    if guards.is_empty() {
        if !task.invariants.is_empty() {
            fields = fields.push_if("checked by", "nothing but the suite");
        }
        return fields;
    }
    for (n, guard) in guards.iter().enumerate() {
        let label = if n == 0 { "checked by" } else { "" };
        fields = match guard.proves.as_deref() {
            Some(proves) => fields
                .push_if(label, proves)
                .push_command("", format!("$ {}", guard.cmd)),
            None => fields.push_command(label, format!("$ {}", guard.cmd)),
        };
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
            // The change's own code, or the shape its task declares in
            // that file: how the work will look, beside what it is about.
            let code = change.code.as_deref().or_else(|| {
                plan.shapes
                    .iter()
                    .find(|shape| shape.owner == task.id && shape.file == change.file())
                    .map(|shape| shape.code.as_str())
            });
            match code {
                Some(code) => Code::whole(at, Some(change.what.clone()), code).into(),
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
            let what = Some("the spec wrote it, and this command runs it".to_string());
            match form {
                Form::Review => Code::reviewed(
                    file.path.as_str(),
                    what,
                    content,
                    CODE_SHOWN,
                    format!("yunta status {run} --node spec"),
                ),
                Form::Whole => Code::whole(file.path.as_str(), what, content),
            }
            .into()
        })
        .collect()
}
