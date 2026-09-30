//! A plan, as the person deciding on it reads it on a terminal: what it
//! changes and why, the choices it makes, the shapes it creates, what it
//! risks and leaves out, the order its tasks run in, then task by task
//! what each does and what a person sees once it is done, what it changes
//! and touches, and what proves it done — in words; the commands stay in
//! the whole plan.

use yunta_core::{ScopeGlob, Task, TasksFile};
use yunta_engine::PlanView;

use crate::render::markdown::{hanging, markdown};
use crate::render::{cell_width, INDENT};

/// Where a section's body sits: one step under its heading, which is one
/// step under the document's own.
const BODY: &str = "    ";

/// The column a task's facts are labelled in, wide enough for the
/// longest label.
const LABEL: usize = "keeps passing".len() + 1;

pub(super) fn plan(file: &TasksFile, of: &str, width: usize) -> Vec<String> {
    let view = PlanView::of(file);
    let steps = view.steps();
    let tasks = yunta_core::text::counted(file.tasks.len(), "task");
    let mut lines = vec![match steps.len() {
        0 | 1 => format!("the plan{of} — {tasks}"),
        n => format!("the plan{of} — {tasks} in {n} steps"),
    }];
    if let Some(summary) = said(&file.summary) {
        lines.extend(hanging(INDENT, "", summary, width));
    }
    if let Some(description) = said(&file.description) {
        lines.push(String::new());
        lines.extend(markdown(description, INDENT, width));
    }
    lines.extend(designed(file, width));
    for (name, items) in [("risks", &file.risks), ("out of scope", &file.out_of_scope)] {
        if !items.is_empty() {
            heading(&mut lines, name);
            for item in items {
                lines.extend(hanging(BODY, "- ", item, width));
            }
        }
    }
    if steps.len() > 1 {
        heading(&mut lines, "order");
        for (at, step) in steps.iter().enumerate() {
            let ids: Vec<&str> = step.iter().map(|task| task.id.as_str()).collect();
            lines.extend(hanging(
                BODY,
                &format!("{}  ", at + 1),
                &ids.join(" · "),
                width,
            ));
        }
    }
    for task in &file.tasks {
        lines.push(String::new());
        lines.extend(card(task, width));
    }
    lines
}

/// What the plan decided, and how it is built: its decisions, then its
/// design and each shape with the task that builds it.
fn designed(file: &TasksFile, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    if !file.decisions.is_empty() {
        heading(&mut lines, "decisions");
        for decision in &file.decisions {
            lines.extend(decided(decision, width));
        }
    }
    if said(&file.design).is_some() || !file.shapes.is_empty() {
        heading(&mut lines, "design");
    }
    if let Some(design) = said(&file.design) {
        lines.extend(markdown(design, BODY, width));
    }
    for shape in &file.shapes {
        let built = format!(
            "{} — built by {}, in {}",
            shape.name, shape.owner, shape.file
        );
        lines.extend(hanging(BODY, "", &built, width));
        lines.extend(markdown(&format!("```\n{}\n```", shape.code), BODY, width));
    }
    lines
}

/// One decision: what was open and what the plan chose, why, and what
/// it did not choose.
fn decided(decision: &yunta_core::Decision, width: usize) -> Vec<String> {
    let mut lines = hanging(
        BODY,
        "- ",
        &format!("{} — {}", decision.question, decision.choice),
        width,
    );
    let under = format!("{BODY}  ");
    if let Some(why) = said(&decision.why) {
        lines.extend(hanging(&under, "because ", why, width));
    }
    if !decision.alternatives.is_empty() {
        lines.extend(hanging(
            &under,
            "rather than ",
            &decision.alternatives.join("; "),
            width,
        ));
    }
    lines
}

/// One task: its id and title, what it does and what a person sees once
/// it is done, and beside each label what it changes and touches, the
/// shapes it builds on, what proves it done, what it keeps passing and
/// what it waits for.
fn card(task: &Task, width: usize) -> Vec<String> {
    let mut lines = hanging(INDENT, &format!("{} — ", task.id), &task.title, width);
    if let Some(description) = said(&task.description) {
        lines.extend(markdown(description, BODY, width));
    }
    if let Some(outcome) = said(&task.outcome) {
        lines.extend(labelled(
            "you will see",
            &[outcome.to_string()],
            Mark::None,
            width,
        ));
    }
    let changes: Vec<String> = task
        .changes
        .iter()
        .map(|change| format!("{}: {}", change.at, change.what))
        .collect();
    lines.extend(labelled("changes", &changes, Mark::Several, width));
    lines.extend(labelled(
        "touches",
        &touches(task, width),
        Mark::None,
        width,
    ));
    lines.extend(labelled("uses", &[task.uses.join(", ")], Mark::None, width));
    lines.extend(labelled(
        "done when",
        &proves(task, false),
        Mark::Several,
        width,
    ));
    lines.extend(labelled(
        "keeps passing",
        &proves(task, true),
        Mark::Several,
        width,
    ));
    if !task.depends_on.is_empty() {
        let after: Vec<&str> = task.depends_on.iter().map(|id| id.as_str()).collect();
        lines.extend(labelled("after", &[after.join(", ")], Mark::None, width));
    }
    lines
}

/// What a task touches: on one line when its globs fit on it, and a
/// directory's to a line when they do not, so no group is cut in two.
fn touches(task: &Task, width: usize) -> Vec<String> {
    let groups = grouped(&task.scope);
    let together = groups.join(", ");
    let room = width.saturating_sub(cell_width(BODY) + LABEL);
    match cell_width(&together) <= room {
        true => vec![together],
        false => groups,
    }
}

/// What each of a task's criteria proves — its guards, or the others —
/// in words, or by its command when it says nothing of what it proves.
fn proves(task: &Task, guards: bool) -> Vec<String> {
    task.criteria
        .iter()
        .filter(|criterion| criterion.is_guard() == guards)
        .map(|criterion| match said(&criterion.proves) {
            Some(proves) => proves.to_string(),
            None => format!("`{}`", criterion.cmd),
        })
        .collect()
}

/// Whether the items beside a label are marked as one of several.
#[derive(Clone, Copy)]
enum Mark {
    /// When there are several: each is a claim of its own.
    Several,
    /// Never: the items are one list, broken across lines.
    None,
}

/// `items` under `label`: the label once, in its column, and each item
/// on its own line beside it.
fn labelled(label: &str, items: &[String], mark: Mark, width: usize) -> Vec<String> {
    let mark = match mark {
        Mark::Several if items.len() > 1 => "- ",
        _ => "",
    };
    let mut lines = Vec::new();
    for (at, item) in items.iter().filter(|item| !item.is_empty()).enumerate() {
        let name = if at == 0 { label } else { "" };
        lines.extend(hanging(BODY, &format!("{name:<LABEL$}{mark}"), item, width));
    }
    lines
}

/// `scope` as a reader scans it: globs that share a directory under that
/// directory once, in the order the task names them.
fn grouped(scope: &[ScopeGlob]) -> Vec<String> {
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

/// A heading of the plan's, a line after what came before it.
fn heading(lines: &mut Vec<String>, name: &str) {
    lines.push(String::new());
    lines.push(format!("{INDENT}{name}"));
}

/// Text that says something, trimmed.
fn said(text: &Option<String>) -> Option<&str> {
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
