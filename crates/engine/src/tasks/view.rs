//! A tasks document as a person reviewing the plan reads it, in Markdown:
//! what the planner wrote for them, and beside it what the engine knows
//! of the plan without being told — the order its tasks run in, what they
//! touch, and what nothing may break.
//!
//! A view of the accepted document: derived from the same bytes every
//! time, so it never says anything the document does not.

use yunta_core::text::escape_mermaid;
use yunta_core::{Task, TaskId, TasksFile};

/// The name the view takes beside the document it is a view of.
pub(crate) const VIEW_NAME: &str = "tasks.md";

/// The whole plan as one Markdown document: what the planner wrote, and
/// beside it what the engine reads off the plan — the order its tasks run
/// in, what they touch, and what nothing may break.
pub(crate) fn markdown(file: &TasksFile) -> String {
    let mut out = format!("# {}\n", said(&file.summary).unwrap_or("The plan"));
    if let Some(description) = said(&file.description) {
        out.push_str(&format!("\n{description}\n"));
    }
    out.push_str(&decisions(&file.decisions));
    if said(&file.design).is_some() || !file.shapes.is_empty() {
        out.push_str("\n## Design\n");
    }
    if let Some(design) = said(&file.design) {
        out.push_str(&format!("\n{design}\n"));
    }
    for shape in &file.shapes {
        out.push_str(&format!(
            "\n### {} — built by {}, in `{}`\n\n```\n{}\n```\n",
            shape.name,
            shape.owner,
            shape.file,
            shape.code.trim_end()
        ));
    }
    out.push_str(&at_a_glance(file));
    out.push_str(&listed("Risks", &file.risks));
    out.push_str(&listed("Out of scope", &file.out_of_scope));
    out.push_str("\n## Tasks\n");
    for task in &file.tasks {
        out.push_str(&task_section(task));
    }
    out
}

/// What the engine reads off the plan: how many tasks, in what order,
/// touching what, holding which guards — and the order drawn, when there
/// is one to draw.
fn at_a_glance(file: &TasksFile) -> String {
    let steps = file.steps();
    let order: Vec<String> = steps
        .iter()
        .map(|step| ids(step.iter().map(|task| &task.id)))
        .collect();
    let mut out = format!(
        "\n## At a glance\n\n- **Tasks:** {}\n- **Order:** {}\n- **Touches:** {}\n",
        file.tasks.len(),
        order.join("; then "),
        code_list(&file.touched())
    );
    let guards = file.guards();
    if !guards.is_empty() {
        out.push_str(&format!(
            "- **Must keep passing:** {}\n",
            code_list(&guards)
        ));
    }
    if steps.len() > 1 {
        out.push_str(&format!("\n```mermaid\n{}```\n", graph(file)));
    }
    out
}

/// Each task, and an arrow from every task it waits on.
fn graph(file: &TasksFile) -> String {
    let mut out = String::from("graph LR\n");
    for task in &file.tasks {
        out.push_str(&format!(
            "  {}[\"{}: {}\"]\n",
            task.id,
            task.id,
            escape_mermaid(&task.title)
        ));
    }
    for task in &file.tasks {
        for before in &task.depends_on {
            out.push_str(&format!("  {before} --> {}\n", task.id));
        }
    }
    out
}

/// Every decision: what was open, what the plan chose and why, and what
/// it did not choose.
fn decisions(decisions: &[yunta_core::Decision]) -> String {
    if decisions.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n## Decisions\n\n");
    for decision in decisions {
        out.push_str(&format!("- **{}** {}", decision.question, decision.choice));
        if let Some(why) = said(&decision.why) {
            out.push_str(&format!(" — because {why}"));
        }
        if !decision.alternatives.is_empty() {
            out.push_str(&format!(
                " (rather than {})",
                decision.alternatives.join("; ")
            ));
        }
        out.push('\n');
    }
    out
}

/// One task: what it does and why, what a person sees once it is done,
/// what it changes and touches, what proves it done, and what it waits
/// for.
fn task_section(task: &Task) -> String {
    let mut out = format!("\n### {} — {}\n", task.id, task.title);
    if let Some(description) = said(&task.description) {
        out.push_str(&format!("\n{description}\n"));
    }
    if let Some(outcome) = said(&task.outcome) {
        out.push_str(&format!("\n**You will see:** {outcome}\n"));
    }
    if !task.changes.is_empty() {
        out.push_str("\n**Changes:**\n\n");
        for change in &task.changes {
            out.push_str(&format!("- `{}` — {}\n", change.at, change.what));
        }
    }
    if !task.uses.is_empty() {
        let uses: Vec<&str> = task.uses.iter().map(String::as_str).collect();
        out.push_str(&format!("\n**Uses:** {}\n", code_list(&uses)));
    }
    if !task.invariants.is_empty() {
        out.push_str("\n**Keeps:**\n\n");
        for invariant in &task.invariants {
            out.push_str(&format!("- {invariant}\n"));
        }
    }
    let scope: Vec<&str> = task.scope.iter().map(|glob| glob.as_str()).collect();
    out.push_str(&format!("\n**Touches:** {}\n", code_list(&scope)));
    out.push_str("\n| Done when | Command |\n|---|---|\n");
    for criterion in &task.criteria {
        let proves = said(&criterion.proves).unwrap_or("—");
        let proves = match criterion.is_guard() {
            true => format!("keeps passing: {proves}"),
            false => proves.to_string(),
        };
        out.push_str(&format!(
            "| {} | `{}` |\n",
            cell(&proves),
            cell(&criterion.cmd)
        ));
    }
    if !task.depends_on.is_empty() {
        out.push_str(&format!("\n**After:** {}\n", ids(task.depends_on.iter())));
    }
    out
}

/// A heading and its items, or nothing when there are none.
fn listed(heading: &str, items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = items.iter().map(|item| format!("- {item}")).collect();
    format!("\n## {heading}\n\n{}\n", lines.join("\n"))
}

/// Text that says something, trimmed.
fn said(text: &Option<String>) -> Option<&str> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn ids<'a>(ids: impl Iterator<Item = &'a TaskId>) -> String {
    ids.map(TaskId::as_str).collect::<Vec<_>>().join(", ")
}

fn code_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("`{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Text for one table cell: on one line, and with the pipe that would
/// end the cell escaped.
fn cell(text: &str) -> String {
    yunta_core::text::one_line(text).replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use yunta_core::shape::Document;
    use yunta_core::TasksFile;

    use super::markdown;

    #[test]
    fn the_whole_plan_says_its_decisions_its_shapes_and_what_each_task_changes() {
        let example: TasksFile =
            yunta_core::shape::read(TasksFile::EXAMPLE.as_bytes(), "example").unwrap();
        let view = markdown(&example);
        for said in [
            "## Decisions",
            "- **Which theme does a new user start with?** Light — because Nothing changes",
            "### Theme — built by add-dark-mode, in `src/theme/mod.rs`",
            "**You will see:** The settings screen has a dark mode switch",
            "- `src/theme/mod.rs::Theme` — the enum, and the setting that holds it",
            "**Uses:** `Theme`",
            "**Keeps:**",
        ] {
            assert!(view.contains(said), "`{said}` is missing from:\n{view}");
        }
        assert!(view.find("## Decisions") < view.find("## Design"));
    }
}
