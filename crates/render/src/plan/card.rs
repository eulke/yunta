//! One task's card: what a person sees once it is done, the files it
//! changes, the shapes of another task it builds on, what it keeps and what
//! tests it; then the code it writes — each shape it builds and each change
//! it makes, once.

use surco::components::{Code, Fields, Section};
use surco::doc::Block;
use surco::line::{Line, Tone};
use yunta_core::shown::{HeldTo, PlanReview, TaskReview};
use yunta_core::{Change, Shape, Task, TasksFile};

use super::{blank, said};

/// One task's card: what it says in words, then the code it writes.
pub(super) fn card(task: &Task, review: &PlanReview) -> Section {
    let judged = judged(review, task);
    let mut blocks: Vec<Block> = vec![facts(task, review, judged).into()];
    for piece in pieces(task, &review.plan, judged) {
        blocks.push(blank());
        blocks.push(piece.code.into());
    }
    Section {
        lead: None,
        title: Line::new()
            .push(Tone::Strong, task.id.as_str())
            .plain(format!(": {}", task.title)),
        blocks,
    }
}

/// How many of the changes `review`'s cards draw show no code, and how
/// many changes they draw.
pub(crate) fn codeless(review: &PlanReview) -> (usize, usize) {
    let drawn: Vec<bool> = review
        .plan
        .tasks
        .iter()
        .flat_map(|task| pieces(task, &review.plan, judged(review, task)))
        .filter(|piece| piece.change.is_some())
        .map(|piece| piece.code.lines.is_empty())
        .collect();
    (
        drawn.iter().filter(|codeless| **codeless).count(),
        drawn.len(),
    )
}

/// How the run judges `task`, when `review` says.
fn judged<'a>(review: &'a PlanReview, task: &Task) -> Option<&'a TaskReview> {
    review.tasks.iter().find(|judged| judged.task == task.id)
}

/// What a task's card says in words: what a person sees once it is done,
/// the files it changes, the shapes of another task it builds on, what it
/// keeps, and what tests it.
fn facts(task: &Task, review: &PlanReview, judged: Option<&TaskReview>) -> Fields {
    let mut fields = Fields::new();
    if let Some(outcome) = said(&task.outcome) {
        fields = fields.push_if("you will see", outcome);
    }
    fields = fields
        .push_if("changes", changed_files(task, judged).join(", "))
        .push_if("builds on", built_on(task, &review.plan).join(", "));
    for (n, invariant) in task.invariants.iter().enumerate() {
        fields = fields.push_if(if n == 0 { "keeps" } else { "" }, invariant.as_str());
    }
    tested(fields, task, review, judged)
}

/// The files `task` changes, each once, in the order it names them. A test
/// the spec wrote is the spec's: a change on one is no change of the
/// task's, and what keeps the plan from being proven says so.
fn changed_files<'a>(task: &'a Task, judged: Option<&TaskReview>) -> Vec<&'a str> {
    let mut files: Vec<&str> = Vec::new();
    for change in task.changes.iter().filter(|change| !denied(change, judged)) {
        if !files.contains(&change.file()) {
            files.push(change.file());
        }
    }
    files
}

/// Whether `change` is on a test the spec wrote, which no session writes.
fn denied(change: &Change, judged: Option<&TaskReview>) -> bool {
    judged.is_some_and(|judged| judged.denied.iter().any(|denied| denied.at == change.at))
}

/// The shapes of another task `task` builds on, each with the task that
/// builds it: a shape it builds itself is in its code.
fn built_on(task: &Task, plan: &TasksFile) -> Vec<String> {
    task.uses
        .iter()
        .filter_map(
            |name| match plan.shapes.iter().find(|shape| shape.name == *name) {
                Some(shape) if shape.owner == task.id => None,
                Some(shape) => Some(format!("{name} (from {})", shape.owner)),
                None => Some(name.clone()),
            },
        )
        .collect()
}

/// `fields` with what tests `task`: each test the spec gives it, what it
/// proves in the spec's words and the command that runs it; with no spec
/// beside the plan, each criterion the plan gives it.
fn tested(
    mut fields: Fields,
    task: &Task,
    review: &PlanReview,
    judged: Option<&TaskReview>,
) -> Fields {
    let tests: Vec<(&str, &str)> = match review.spec.as_ref().and_then(|spec| spec.of(&task.id)) {
        Some(spec) => spec
            .tests
            .iter()
            .map(|test| (test.proves.as_str(), test.cmd.as_str()))
            .collect(),
        None => judged
            .into_iter()
            .flat_map(|judged| &judged.criteria)
            .filter(|criterion| matches!(criterion.from, HeldTo::Plan))
            .map(|criterion| {
                (
                    criterion.proves.as_deref().unwrap_or_default(),
                    criterion.cmd.as_str(),
                )
            })
            .collect(),
    };
    for (n, (proves, cmd)) in tests.into_iter().enumerate() {
        let label = if n == 0 { "tested by" } else { "" };
        fields = match proves.trim() {
            "" => fields.push_command(label, cmd),
            proves => fields.push_if(label, proves).push_command("", cmd),
        };
    }
    fields
}

/// One piece of the code a task writes: the change it is drawn under, when
/// it is one's, and its code.
struct Piece<'a> {
    change: Option<&'a Change>,
    code: Code,
}

/// The code `task` writes, each piece once. A shape whose file no change
/// names comes first. Under each change, the shape it is about, said with
/// what the change is for, when the change brings no lines of its own or
/// only a shape's; its own lines otherwise, after that shape; and with
/// neither, where it is and what for. A shape no change is about comes
/// last.
fn pieces<'a>(task: &'a Task, plan: &'a TasksFile, judged: Option<&TaskReview>) -> Vec<Piece<'a>> {
    let owned: Vec<&Shape> = plan
        .shapes
        .iter()
        .filter(|shape| shape.owner == task.id)
        .collect();
    let changes: Vec<&Change> = task
        .changes
        .iter()
        .filter(|change| !denied(change, judged))
        .collect();
    let mut drawn = vec![false; owned.len()];
    let mut pieces = Vec::new();
    for (at, shape) in owned.iter().enumerate() {
        if !changes.iter().any(|change| shape.file == change.file()) {
            drawn[at] = true;
            pieces.push(Piece {
                change: None,
                code: shaped(shape, None),
            });
        }
    }
    for change in changes {
        pieces.extend(under(change, &owned, &mut drawn));
    }
    for (at, shape) in owned.iter().enumerate() {
        if !drawn[at] {
            pieces.push(Piece {
                change: None,
                code: shaped(shape, None),
            });
        }
    }
    pieces
}

/// What a card draws under `change`: the shape among `owned` it is about,
/// said with what the change is for when it brings no lines of its own or
/// only a shape's; then its own lines; and with neither, where it is and
/// what for. A shape drawn is marked in `drawn`.
fn under<'a>(change: &'a Change, owned: &[&Shape], drawn: &mut [bool]) -> Vec<Piece<'a>> {
    let about = about(change, owned, drawn);
    let own = change
        .code
        .as_deref()
        .filter(|code| !owned.iter().any(|shape| shape.code.trim() == code.trim()));
    let mut pieces = Vec::new();
    if let Some(at) = about {
        drawn[at] = true;
        pieces.push(Piece {
            change: own.is_none().then_some(change),
            code: shaped(owned[at], own.is_none().then(|| change.what.clone())),
        });
    }
    match (own, about) {
        (Some(code), _) => pieces.push(Piece {
            change: Some(change),
            code: placed(change, code),
        }),
        (None, Some(_)) => {}
        (None, None) => pieces.push(Piece {
            change: Some(change),
            code: placed(change, ""),
        }),
    }
    pieces
}

/// The shape of its task `change` is about, by its place in `owned`, when
/// it has not been drawn yet: the one whose code the change repeats, else
/// the one it names, else, for a change to its file as a whole, the first
/// of its file.
fn about(change: &Change, owned: &[&Shape], drawn: &[bool]) -> Option<usize> {
    let open = |at: &usize| !drawn[*at] && owned[*at].file == change.file();
    let repeated = change.code.as_deref().and_then(|code| {
        (0..owned.len())
            .filter(open)
            .find(|at| owned[*at].code.trim() == code.trim())
    });
    let named = || {
        let names: Vec<&str> = change
            .within()
            .last()?
            .split(['/', ','])
            .map(str::trim)
            .collect();
        (0..owned.len())
            .filter(open)
            .find(|at| names.contains(&owned[*at].name.as_str()))
    };
    let whole = || match change.within().next() {
        None => (0..owned.len()).find(open),
        Some(_) => None,
    };
    repeated.or_else(named).or_else(whole)
}

/// A shape, whole, titled by its file and its name; `what` is what the
/// change it stands under is for.
fn shaped(shape: &Shape, what: Option<String>) -> Code {
    Code::whole(shape.file.as_str(), what, &shape.code).within([shape.name.as_str()])
}

/// `code` where `change` is, with what it is for.
fn placed(change: &Change, code: &str) -> Code {
    Code::whole(change.file(), Some(change.what.clone()), code).within(change.within())
}
