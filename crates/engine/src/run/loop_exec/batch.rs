//! Which tasks one iteration of a loop works on at once.

use yunta_core::events::TaskStatus;
use yunta_core::{ScopeGlob, Task, TasksFile};

use crate::replay::RunState;

/// Up to `concurrency` tasks this iteration may work on, in
/// declaration order: a task whose dependencies are all `Done` and is
/// itself still `Pending`, or an orphaned `Running` task with no
/// terminal event after it (a crash mid-batch — orphaned tasks always
/// get re-run on resume).
///
/// Two tasks whose reaches might meet never share a batch. Their
/// declared scopes cannot — `tasks::register` refuses two tasks with no
/// `depends_on` path between them that declare overlapping ones — but
/// what the run lets a task write beyond its scope can cross into
/// another's: a file that names a shape one owns may lie in the other's
/// scope. The later task waits for a following batch, and works on the
/// tree the first one landed in.
pub(super) fn select_batch<'a>(
    tasks: &'a TasksFile,
    state: &RunState,
    concurrency: u32,
) -> Vec<&'a Task> {
    let mut batch: Vec<(&Task, Vec<ScopeGlob>)> = Vec::new();
    for task in tasks.tasks.iter().filter(|task| ready(task, state)) {
        if batch.len() >= concurrency as usize {
            break;
        }
        let reach = reach(task, state);
        if batch.iter().any(|(_, chosen)| meet(chosen, &reach)) {
            continue;
        }
        batch.push((task, reach));
    }
    batch.into_iter().map(|(task, _)| task).collect()
}

/// Whether `task` may be worked on now.
fn ready(task: &Task, state: &RunState) -> bool {
    match state.tasks.status(&task.id) {
        Some(TaskStatus::Pending) => task
            .depends_on
            .iter()
            .all(|dep| state.tasks.status(dep) == Some(TaskStatus::Done)),
        Some(TaskStatus::Running) => true,
        _ => false,
    }
}

/// Everything `task` may write: what it declared, and what the log lets
/// it reach beyond that.
fn reach(task: &Task, state: &RunState) -> Vec<ScopeGlob> {
    task.scope
        .iter()
        .cloned()
        .chain(state.grants.reach_for(&task.id))
        .collect()
}

/// Whether two reaches might select the same file.
fn meet(a: &[ScopeGlob], b: &[ScopeGlob]) -> bool {
    a.iter().any(|glob| yunta_core::reaches_any(glob, b))
}

#[cfg(test)]
mod tests {
    use yunta_core::events::{
        EventPayload, ScopeDerivedPayload, ScopeEvent, TaskEvent, TaskRegisteredPayload,
    };
    use yunta_core::{ScopeGlob, TasksFile};
    use yunta_testkit_core::Log;

    use super::select_batch;

    fn document() -> TasksFile {
        yunta_testkit::tasks_document(&[("task-h", "h.rs", "true"), ("task-i", "i.rs", "true")])
    }

    fn registered(log: Log, document: &TasksFile) -> Log {
        document.tasks.iter().fold(log, |log, task| {
            log.node(
                "plan",
                EventPayload::Tasks(TaskEvent::Registered(TaskRegisteredPayload {
                    task_id: task.id.clone(),
                    criteria: task.criteria.iter().map(Into::into).collect(),
                    scope: task.scope.clone(),
                    depends_on: Vec::new(),
                })),
            )
        })
    }

    #[test]
    fn overlapping_reach_never_shares_a_batch() {
        let document = document();
        let derived = EventPayload::Scope(ScopeEvent::Derived(ScopeDerivedPayload {
            task_id: "task-h".parse().unwrap(),
            paths: vec![ScopeGlob::from("i.rs")],
            shapes: vec!["build".into()],
            common: Vec::new(),
            at: "a".repeat(40).parse().unwrap(),
        }));
        let apart = crate::replay::derive(&registered(Log::for_run("r"), &document).build());
        let meeting = crate::replay::derive(
            &registered(Log::for_run("r"), &document)
                .node("plan", derived)
                .build(),
        );

        assert_eq!(select_batch(&document, &apart, 2).len(), 2);
        let batch: Vec<&str> = select_batch(&document, &meeting, 2)
            .iter()
            .map(|task| task.id.as_str())
            .collect();
        assert_eq!(batch, ["task-h"]);
    }
}
