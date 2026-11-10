//! `yunta schema`: the shape of a document Yunta reads and validates,
//! for whoever has to write one.
//!
//! This is the door for a shell — an agent working in a repo without the
//! control plane mounted, or a person writing the file by hand. It is
//! the only way either of them learns the format: `crates/core/schemas/`
//! is a development artifact of this repository, and whoever installed
//! the binary has no repository to read it from.
//!
//! The text is the same one a node's session receives and the
//! `document_shape` tool returns, from the same constant. Four doors
//! rendering four texts drift apart at the first schema change; four
//! doors rendering one do not.

use surco::look::Look;
use yunta_core::shape::contract;
use yunta_core::step::NextStep;
use yunta_core::DocumentKind;
use yunta_render::command;

use crate::error::CliError;

use crate::output::Outcome;
use crate::output::{raw, show, Mode};
use yunta_render::page::Schema;

/// Prints one document's shape, or lists the documents when none is named.
pub fn schema(name: Option<&str>, output: Mode, look: &Look) -> Result<Outcome, CliError> {
    let Some(name) = name else {
        if output == Mode::Json {
            return Err(CliError::msg(format!(
                "`--json` emits one document's schema: name a document, or run `{}` with no \
                 arguments to list them",
                command::line(&NextStep::Schemas)
            )));
        }
        show(look, &Schema::Documents);
        return Ok(Outcome::Success);
    };
    // One parse and one sentence: a name no document has is answered with
    // every name there is, and the one it is a slip from.
    match (name.parse::<DocumentKind>()?, output) {
        (DocumentKind::Workflow, Mode::Json) => raw(yunta_core::schema::workflow_json()),
        (DocumentKind::Workflow, _) => show(look, &Schema::WorkflowKeys),
        // The committed bytes CI proves still match the types (`cargo
        // xtask schema --check`), never a schema generated here:
        // generating it would pull `schemars` and the whole
        // schema-building machinery into the shipped binary — around
        // 100 KB, reachable from this one flag — against a recorded
        // binary-size ceiling.
        (DocumentKind::Artifact(kind), Mode::Json) => raw(yunta_core::schema::json(kind)),
        (DocumentKind::Artifact(kind), _) => raw(&contract(kind)),
    }
    Ok(Outcome::Success)
}
