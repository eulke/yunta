//! What the tree a plan was registered against held of each of its shapes,
//! as the reading of the tree found it: new where the shape's file does
//! not name it, and otherwise what the file holds of it and who else names
//! it beyond the files the plan changes.

use std::collections::BTreeSet;

use surco::code::Language;
use yunta_core::events::{
    ArtifactId, EventPayload, InTree, Outside, ShapeRead, ShapesReadPayload, TaskEvent,
};
use yunta_core::{ArtifactKind, NodeId, Shape, TasksFile};

use super::{Named, Reading};
use crate::replay::RunState;
use crate::run::RunError;
use crate::run_log::RunLog;

/// States what the tree held of `document`'s shapes, for the plan this
/// registration follows: the tasks document `node` accepted last, or the
/// last the run accepted where no node brought it in. A reading the log
/// already holds for that plan, the same in every part as the log stores
/// this one, is not stated again.
pub(super) async fn state(
    log: &RunLog<'_>,
    node: Option<&NodeId>,
    document: &TasksFile,
    reading: &Reading,
    state: &RunState,
) -> Result<(), RunError> {
    let tasks = ArtifactId::Interpreted {
        kind: ArtifactKind::Tasks,
    };
    let Some(plan) = state.artifacts.latest(&tasks, node) else {
        return Ok(());
    };
    let shapes = shapes_of(document, reading);
    if shapes.is_empty() {
        return Ok(());
    }
    let read = EventPayload::Tasks(TaskEvent::ShapesRead(ShapesReadPayload {
        plan: plan.content_hash.clone(),
        at: reading.at.clone(),
        shapes,
    }));
    if let Some(held) = state.tasks.shapes_read(&plan.content_hash) {
        // What the log holds had its secrets taken out on the way in.
        let held = EventPayload::Tasks(TaskEvent::ShapesRead(held.clone()));
        if log.redacted(read.clone())? == held {
            return Ok(());
        }
    }
    log.record(node, read).await?;
    Ok(())
}

/// Each shape of `document` the reading followed, in the plan's order, as
/// the tree held it.
fn shapes_of(document: &TasksFile, reading: &Reading) -> Vec<ShapeRead> {
    // Every file a task of the plan changes, a change the spec denies
    // among them: what names a shape there is the plan's own work.
    let changed: BTreeSet<String> = document
        .tasks
        .iter()
        .flat_map(|task| &task.changes)
        .map(|change| yunta_core::in_repo(change.file()))
        .collect();
    document
        .shapes
        .iter()
        .filter_map(|shape| {
            let named = reading.named(&shape.name)?;
            Some(ShapeRead {
                name: shape.name.clone(),
                owner: shape.owner.clone(),
                file: yunta_core::in_repo(&shape.file),
                in_tree: in_tree(shape, named, reading, &changed),
            })
        })
        .collect()
}

/// What `shape`'s file held of it, `named` saying who names it: new where
/// its file's code does not; otherwise today's code of it, read the way
/// the shape writes it, and the files beyond `changed` whose code names it.
fn in_tree(shape: &Shape, named: &Named, reading: &Reading, changed: &BTreeSet<String>) -> InTree {
    let file = yunta_core::in_repo(&shape.file);
    if !named.in_own(&file) {
        return InTree::New;
    }
    let outside = match named {
        Named::Files(files) => Outside::Files {
            paths: files
                .iter()
                .filter(|path| **path != file && !changed.contains(*path))
                .cloned()
                .collect(),
        },
        Named::Common { files, .. } => Outside::Common { files: *files },
    };
    // A file's items are read as the shape writes them, in each language
    // surco's item reader reads.
    let code = Language::of(&file).and_then(|language| {
        reading
            .source(&file)
            .and_then(|source| super::code_words::items::as_planned(source, &shape.code, language))
    });
    InTree::There { code, outside }
}
