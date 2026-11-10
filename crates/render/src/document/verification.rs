//! See [`super`]. The verdict on a run's evidence as `verify --json` carries
//! it: whether every check holds, the word the headline says, and each check
//! in the order it ran.

use surco::components::Check;
use yunta_core::RunId;

use super::SCHEMA_VERSION;
use crate::page::Integrity;

/// The verdict on a run's evidence as a program reads it: whether every
/// check holds, the word the headline says, and each check in the order it
/// ran.
///
/// Not the verified page, which a person reads drawn from the same checks.
pub struct VerificationDocument<'a> {
    pub run_id: &'a RunId,
    pub checks: &'a [Check],
}

impl serde::Serialize for VerificationDocument<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let integrity = Integrity::of(self.checks);
        Written {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id,
            holds: integrity.holds(),
            word: integrity.word(),
            checks: self.checks,
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
    holds: bool,
    /// The word the headline says: `intact` when every check holds, `broken`
    /// otherwise.
    word: &'static str,
    #[serde(serialize_with = "rows")]
    checks: &'a [Check],
}

/// Each check as the document a program reads.
fn rows<S: serde::Serializer>(checks: &&[Check], serializer: S) -> Result<S::Ok, S::Error> {
    super::check::rows(checks, serializer)
}
