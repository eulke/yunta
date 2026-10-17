//! The order a plan's tasks run in, and what the whole plan touches and
//! keeps passing — what any reader of a plan is told without the planner
//! writing it down.

use std::collections::HashMap;

use super::{Task, TasksFile};
use crate::TaskId;

impl TasksFile {
    /// The tasks of each step, in document order: the first step waits
    /// on nothing, and each later one on a task of the step before it.
    pub fn steps(&self) -> Vec<Vec<&Task>> {
        let mut steps: Vec<Vec<&Task>> = Vec::new();
        let mut depth: HashMap<&TaskId, usize> = HashMap::new();
        for task in &self.tasks {
            let at = step_of(task, self, &mut depth, 0);
            if steps.len() <= at {
                steps.resize(at + 1, Vec::new());
            }
            if let Some(step) = steps.get_mut(at) {
                step.push(task);
            }
        }
        steps
    }

    /// Every scope glob a task names, once, in the order they are named.
    pub fn touched(&self) -> Vec<&str> {
        let mut touched: Vec<&str> = Vec::new();
        for glob in self.tasks.iter().flat_map(|task| &task.scope) {
            if !touched.contains(&glob.as_str()) {
                touched.push(glob.as_str());
            }
        }
        touched
    }

    /// Every guard a task holds, once, in the order they are named.
    pub fn guards(&self) -> Vec<&str> {
        let mut guards: Vec<&str> = Vec::new();
        for criterion in self.tasks.iter().flat_map(|task| &task.criteria) {
            if criterion.is_guard() && !guards.contains(&criterion.cmd.as_str()) {
                guards.push(&criterion.cmd);
            }
        }
        guards
    }
}

/// The step `task` runs in: one after the latest of the tasks it waits
/// on. The document's own rules refuse a cycle before anyone orders it;
/// `seen` stops one anyway, rather than recursing forever.
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

#[cfg(test)]
mod tests {
    use crate::TasksFile;

    fn plan(yaml: &str) -> TasksFile {
        crate::yaml::parse(yaml).unwrap()
    }

    #[test]
    fn a_task_runs_one_step_after_the_latest_task_it_waits_on() {
        let file = plan(
            r#"
tasks:
  - { id: a, title: A, scope: [a.rs], criteria: [{ cmd: "true" }] }
  - { id: b, title: B, scope: [b.rs], criteria: [{ cmd: "true" }], depends_on: [a] }
  - { id: c, title: C, scope: [c.rs], criteria: [{ cmd: "true" }], depends_on: [a] }
  - { id: d, title: D, scope: [d.rs], criteria: [{ cmd: "true" }], depends_on: [b, c] }
"#,
        );
        let steps: Vec<Vec<&str>> = file
            .steps()
            .iter()
            .map(|step| step.iter().map(|task| task.id.as_str()).collect())
            .collect();
        assert_eq!(steps, vec![vec!["a"], vec!["b", "c"], vec!["d"]]);
    }

    #[test]
    fn what_a_plan_touches_and_keeps_passing_is_named_once() {
        let file = plan(
            r#"
tasks:
  - { id: a, title: A, scope: [a.rs, x.rs], criteria: [{ cmd: "true" }, { cmd: "cargo test", type: guard }] }
  - { id: b, title: B, scope: [x.rs, b.rs], criteria: [{ cmd: "cargo test", type: guard }] }
"#,
        );
        assert_eq!(file.touched(), vec!["a.rs", "x.rs", "b.rs"]);
        assert_eq!(file.guards(), vec!["cargo test"]);
    }
}
