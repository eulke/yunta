//! The field as `list --runs --json` carries it: where the project keeps its
//! runs, each run's facts and the words render says them in, in the order a
//! reader has always read them. A document reads the same whichever terminal
//! asked for it: its words take Unicode, as a run's document does.

use std::path::Path;
use std::time::Duration;

use crate::page::{FieldGroup, FieldRow, UnreadableRun};
use surco::glyphs::Glyphs;
use surco::words::Standing;
use yunta_core::{ModeName, RunId, WorkflowName};

use super::SCHEMA_VERSION;

/// The field as a program reads it: where the project keeps its runs, the
/// runs it lists, the ones it can name but not read, and how many it leaves
/// to other projects.
///
/// Not the field page, which a person reads drawn from the same runs.
pub struct FieldDocument<'a> {
    /// Where the project keeps its runs, which a field with none names.
    pub storage: &'a Path,
    pub rows: &'a [FieldRow],
    /// The runs it can name but not read.
    pub unreadable: &'a [UnreadableRun],
    /// How many runs it leaves to other projects.
    pub elsewhere: usize,
}

impl serde::Serialize for FieldDocument<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Written {
            schema_version: SCHEMA_VERSION,
            storage: self.storage,
            rows: self.rows.iter().map(Row::of).collect(),
            unreadable: self.unreadable,
            elsewhere: self.elsewhere,
        }
        .serialize(serializer)
    }
}

/// The document as it is written, under the schema version every document
/// shares.
#[derive(serde::Serialize)]
struct Written<'a> {
    schema_version: u32,
    storage: &'a Path,
    rows: Vec<Row<'a>>,
    unreadable: &'a [UnreadableRun],
    elsewhere: usize,
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
        Row {
            run_id: &frame.run_id,
            group: row.group(),
            word: frame.standing,
            workflow: &frame.workflow,
            mode: &frame.mode,
            age: row.stood(),
            reason: row.reason(Glyphs::Unicode),
            command: row.command(),
            said: row.said(Glyphs::Unicode),
            furrow: frame.nodes.iter().map(|node| node.standing).collect(),
            asking: row.asking(),
        }
    }
}

/// A run's age in seconds, as a JSON reader compares it.
fn as_secs<S: serde::Serializer>(age: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(age.as_secs_f64())
}
