//! The field as `list --runs --json` carries it: each row's facts, and the
//! words render says them in, in the order a reader has always read them.

use std::time::Duration;

use surco::words::Standing;
use yunta_core::{ModeName, RunId, WorkflowName};
use yunta_render::page::{FieldGroup, FieldRow};

/// Every row of a field, each as the document a program reads.
pub(crate) fn rows<S: serde::Serializer>(
    rows: &[FieldRow],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(rows.iter().map(Row::of))
}

/// One run of a field, as a program reads it: what the run is, the group it
/// is listed in, how long it has stood there, what holds it and what moves
/// it, in the words a person reads them in.
#[derive(serde::Serialize)]
struct Row<'a> {
    run_id: &'a RunId,
    group: FieldGroup,
    word: Standing,
    workflow: &'a WorkflowName,
    mode: &'a ModeName,
    #[serde(rename = "age_secs", serialize_with = "as_secs")]
    age: Duration,
    reason: Option<String>,
    command: Option<String>,
    said: String,
    furrow: Vec<Standing>,
    asking: bool,
}

impl<'a> Row<'a> {
    fn of(row: &'a FieldRow) -> Self {
        let frame = &row.frame;
        let glyphs = crate::output::glyphs();
        Row {
            run_id: &frame.run_id,
            group: row.group(),
            word: frame.standing,
            workflow: &frame.workflow,
            mode: &frame.mode,
            age: row.stood(),
            reason: row.reason(glyphs),
            command: row.command(),
            said: row.said(glyphs),
            furrow: frame.nodes.iter().map(|node| node.standing).collect(),
            asking: row.asking(),
        }
    }
}

/// A run's age in seconds, as a JSON reader compares it.
fn as_secs<S: serde::Serializer>(age: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(age.as_secs_f64())
}
