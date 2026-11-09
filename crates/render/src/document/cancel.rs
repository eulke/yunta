//! See [`super`]. What a cancel did, as `cancel --json` carries it: the run,
//! and how the cancel ended with what it did to the processes left.

use yunta_core::RunId;

use super::SCHEMA_VERSION;
use crate::page::CancelState;

/// What a cancel did, as a program reads it.
///
/// Not the cancelled page, which a person reads drawn from the same state.
pub struct CancelDocument<'a> {
    pub run_id: &'a RunId,
    pub state: &'a CancelState,
}

impl serde::Serialize for CancelDocument<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Written {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id,
            state: self.state,
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
    #[serde(flatten)]
    state: &'a CancelState,
}
