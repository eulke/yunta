//! A tasks document as a person reviewing the plan reads it, in Markdown:
//! what the planner wrote for them, and beside it what the engine knows
//! of the plan without being told — the order its tasks run in, what they
//! touch, and what nothing may break.
//!
//! A view of the accepted document, like `progress.md` is of the log:
//! derived from the same bytes every time, so it never says anything the
//! document does not.

use std::collections::HashMap;

use yunta_core::text::escape_mermaid;
use yunta_core::{Task, TaskId, TasksFile};

/// The name the view takes beside the document it is a view of.
pub(crate) const VIEW_NAME: &str = "tasks.md";

/// The plan, read for a person: its tasks by the step they run in, what
/// they touch together, and the guards they hold. One reading, drawn as
/// Markdown beside the document and on the terminal a decision is made
/// at, so the two never say different things about the same plan.
pub struct PlanView<'a> {
    file: &'a TasksFile,
    /// The tasks of each step, in document order: the first step waits
    /// on nothing, and each later one on a task of the step before it.
    steps: Vec<Vec<&'a Task>>,
    touched: Vec<&'a str>,
    guards: Vec<&'a str>,
}

impl<'a> PlanView<'a> {
    pub fn of(file: &'a TasksFile) -> Self {
        let mut steps: Vec<Vec<&Task>> = Vec::new();
        let mut depth: HashMap<&TaskId, usize> = HashMap::new();
        for task in &file.tasks {
            let at = step_of(task, file, &mut depth, 0);
            if steps.len() <= at {
                steps.resize(at + 1, Vec::new());
            }
            if let Some(step) = steps.get_mut(at) {
                step.push(task);
            }
        }
        let mut touched: Vec<&str> = Vec::new();
        let mut guards: Vec<&str> = Vec::new();
        for task in &file.tasks {
            for glob in &task.scope {
                if !touched.contains(&glob.as_str()) {
                    touched.push(glob.as_str());
                }
            }
            for criterion in task.criteria.iter().filter(|c| c.is_guard()) {
                if !guards.contains(&criterion.cmd.as_str()) {
                    guards.push(&criterion.cmd);
                }
            }
        }
        PlanView {
            file,
            steps,
            touched,
            guards,
        }
    }

    /// The tasks of each step, in document order: the first step waits
    /// on nothing, and each later one on a task of the step before it.
    pub fn steps(&self) -> &[Vec<&'a Task>] {
        &self.steps
    }

    /// The whole plan as one Markdown document.
    pub(crate) fn markdown(&self) -> String {
        let file = self.file;
        let mut out = format!("# {}\n", said(&file.summary).unwrap_or("The plan"));
        if let Some(description) = said(&file.description) {
            out.push_str(&format!("\n{description}\n"));
        }
        if let Some(design) = said(&file.design) {
            out.push_str(&format!("\n## Design\n\n{design}\n"));
        }
        out.push_str(&self.at_a_glance());
        out.push_str(&listed("Risks", &file.risks));
        out.push_str(&listed("Out of scope", &file.out_of_scope));
        out.push_str("\n## Tasks\n");
        for task in &file.tasks {
            out.push_str(&task_section(task));
        }
        out
    }

    /// What the engine reads off the plan: how many tasks, in what order,
    /// touching what, holding which guards — and the order drawn, when
    /// there is one to draw.
    fn at_a_glance(&self) -> String {
        let order: Vec<String> = self
            .steps
            .iter()
            .map(|step| ids(step.iter().map(|task| &task.id)))
            .collect();
        let mut out = format!(
            "\n## At a glance\n\n- **Tasks:** {}\n- **Order:** {}\n- **Touches:** {}\n",
            self.file.tasks.len(),
            order.join("; then "),
            code_list(&self.touched)
        );
        if !self.guards.is_empty() {
            out.push_str(&format!(
                "- **Must keep passing:** {}\n",
                code_list(&self.guards)
            ));
        }
        if self.steps.len() > 1 {
            out.push_str(&format!("\n```mermaid\n{}```\n", self.graph()));
        }
        out
    }

    /// Each task, and an arrow from every task it waits on.
    fn graph(&self) -> String {
        let mut out = String::from("graph LR\n");
        for task in &self.file.tasks {
            out.push_str(&format!(
                "  {}[\"{}: {}\"]\n",
                task.id,
                task.id,
                escape_mermaid(&task.title)
            ));
        }
        for task in &self.file.tasks {
            for before in &task.depends_on {
                out.push_str(&format!("  {before} --> {}\n", task.id));
            }
        }
        out
    }
}

/// The step `task` runs in: one after the latest of the tasks it waits
/// on. The document's own rules refuse a cycle before a view is ever
/// drawn; `seen` stops one anyway, rather than recursing forever.
fn step_of<'a>(
    task: &'a Task,
    file: &'a TasksFile,
    depth: &mut HashMap<&'a TaskId, usize>,
    seen: usize,
) -> usize {
    if let Some(known) = depth.get(&task.id) {
        return *known;
    }
    if seen > file.tasks.len() {
        return 0;
    }
    let at = task
        .depends_on
        .iter()
        .filter_map(|id| file.tasks.iter().find(|other| other.id == *id))
        .map(|before| step_of(before, file, depth, seen + 1) + 1)
        .max()
        .unwrap_or(0);
    depth.insert(&task.id, at);
    at
}

/// One task: what it does and why, what it touches, what proves it done,
/// and what it waits for.
fn task_section(task: &Task) -> String {
    let mut out = format!("\n### {} — {}\n", task.id, task.title);
    if let Some(description) = said(&task.description) {
        out.push_str(&format!("\n{description}\n"));
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
