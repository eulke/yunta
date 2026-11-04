//! What a task may write beyond the scope it declared: each file whose
//! code names a shape it owns, as the reading of the tree found it, and
//! the shapes it owns that are named too widely to follow.

use std::collections::BTreeSet;
use std::path::Path;

use yunta_core::events::{EventPayload, GrantLedger, ScopeDerivedPayload, ScopeEvent};
use yunta_core::{NodeId, ScopeGlob, Shape, Task, TasksFile};

use super::{Named, Reading};
use crate::run::RunError;
use crate::run_log::RunLog;

/// States, for each task of `document`, every file the reading found whose
/// code names a shape the task owns and its declared scope leaves out.
/// Once per change: a task whose reach `stated` already holds as the log
/// stores this one is left alone, and one a recut gave other shapes is
/// stated again, replacing it.
pub(super) async fn state(
    log: &RunLog<'_>,
    node: Option<&NodeId>,
    document: &TasksFile,
    reading: &Reading,
    stated: &GrantLedger,
) -> Result<(), RunError> {
    for task in &document.tasks {
        let reach = reach_of(task, &document.shapes, reading);
        let unchanged = match stated.derived_for(&task.id) {
            // Compared as the log stores it, its secrets taken out, at
            // whichever commit either was read.
            Some(before) => {
                let again = ScopeDerivedPayload {
                    at: before.at.clone(),
                    ..reach.clone()
                };
                log.redacted(derived(again))? == derived(before.clone())
            }
            None => reach.paths.is_empty() && reach.common.is_empty(),
        };
        if unchanged {
            continue;
        }
        log.record(node, derived(reach)).await?;
    }
    Ok(())
}

/// The event that states `reach`.
fn derived(reach: ScopeDerivedPayload) -> EventPayload {
    EventPayload::Scope(ScopeEvent::Derived(reach))
}

/// The files the reading found naming a shape `task` owns, outside its
/// declared scope, and the shapes named too widely to follow.
fn reach_of(task: &Task, shapes: &[Shape], reading: &Reading) -> ScopeDerivedPayload {
    let declared = yunta_core::scope_globset(&task.scope).ok();
    let mut files = BTreeSet::new();
    let mut named = Vec::new();
    let mut common = Vec::new();
    for shape in shapes.iter().filter(|shape| shape.owner == task.id) {
        match reading.named(&shape.name) {
            None => {}
            Some(Named::Common { .. }) => common.push(shape.name.clone()),
            Some(Named::Files(naming)) => {
                let beyond: Vec<&String> = naming
                    .iter()
                    .filter(|file| !declared.as_ref().is_some_and(|set| set.is_match(file)))
                    .collect();
                if !beyond.is_empty() {
                    named.push(shape.name.clone());
                    files.extend(beyond.into_iter().cloned());
                }
            }
        }
    }
    ScopeDerivedPayload {
        task_id: task.id.clone(),
        paths: files
            .iter()
            .filter_map(|file| ScopeGlob::exact(Path::new(file)).ok())
            .collect(),
        shapes: named,
        common,
        at: reading.at.clone(),
    }
}
