//! See [`super`]. What `doctor` found, as `doctor --json` carries it: whether
//! `--session` opened a session per binding, whether the machine is ready,
//! and every check in the order it ran, each with what it means and how to
//! make it hold when the check knows.

use surco::components::Check;

use super::SCHEMA_VERSION;
use crate::page::doctor::{checks, Finding, Readiness};

/// What `doctor` found, as a program reads it.
///
/// Not the doctor page, which a person reads drawn from the same checks.
pub struct DoctorDocument<'a> {
    /// Whether `--session` opened a session per binding.
    pub session: bool,
    pub findings: &'a [Finding],
}

impl serde::Serialize for DoctorDocument<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let readiness = Readiness::of(self.findings);
        Written {
            schema_version: SCHEMA_VERSION,
            session: self.session,
            holds: readiness.holds(),
            word: readiness.word(),
            checks: &checks(self.findings),
        }
        .serialize(serializer)
    }
}

/// The document as it is written, under the schema version every document
/// shares.
#[derive(serde::Serialize)]
struct Written<'a> {
    schema_version: u32,
    session: bool,
    holds: bool,
    /// The word the headline says: `ready` when every check holds, `not
    /// ready` otherwise.
    word: &'static str,
    #[serde(serialize_with = "super::check::rows")]
    checks: &'a [Check],
}
