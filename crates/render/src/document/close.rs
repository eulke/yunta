//! See [`super`]. What a close did, as `close --json` carries it: the run,
//! the word it stands in, and who closed it.

use surco::words::Standing;
use yunta_core::{Responder, RunId};

use super::SCHEMA_VERSION;

/// What a close did, as a program reads it.
///
/// Not the closed page, which a person reads drawn from the same close.
pub struct CloseDocument<'a> {
    pub run_id: &'a RunId,
    pub word: Standing,
    pub by: &'a Responder,
}

impl serde::Serialize for CloseDocument<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Written {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id,
            word: self.word,
            by: self.by,
        }
        .serialize(serializer)
    }
}

/// The document as it is written, under the schema version every document
/// shares.
#[derive(serde::Serialize)]
struct Written<'a> {
    schema_version: u32,
    run_id: &'a RunId,
    word: Standing,
    by: &'a Responder,
}
