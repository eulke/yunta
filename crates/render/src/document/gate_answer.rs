//! See [`super`]. What answering a gate did, as `resolve-gate --json` and
//! `review --json` carry it: the run, and the decision recorded, declined or
//! interrupted.

use yunta_core::RunId;

use super::SCHEMA_VERSION;
use crate::page::Resolution;

/// What answering a gate did, as a program reads it.
///
/// Not the gate-answered page, which a person reads drawn from the same
/// resolution.
pub struct GateAnswerDocument<'a> {
    pub run_id: &'a RunId,
    pub resolution: &'a Resolution,
}

impl serde::Serialize for GateAnswerDocument<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Written {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id,
            resolution: self.resolution,
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
    resolution: &'a Resolution,
}
