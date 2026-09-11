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

use yunta_core::shape::published;
use yunta_core::ArtifactKind;

use crate::error::{CliError, Outcome};

/// Prints one kind's shape, or lists the kinds when none is named.
pub fn schema(kind: Option<&str>, json: bool) -> Result<Outcome, CliError> {
    let Some(name) = kind else {
        if json {
            return Err(CliError::msg(
                "`--json` emits one document's schema: name a kind, or run `yunta schema` \
                 with no arguments to list them",
            ));
        }
        println!("{}", list());
        return Ok(Outcome::Success);
    };

    // One parse and one sentence: `ArtifactKind`'s own `FromStr` names
    // the kinds that exist, so this door and the `document_shape` tool
    // answer the same mistake with the same words.
    let kind = name
        .parse::<ArtifactKind>()
        .map_err(|e| CliError::msg(e.to_string()))?;
    if json {
        // The committed bytes CI proves still match the types (`cargo
        // xtask schema --check`), never a schema generated here:
        // generating it would pull `schemars` and the whole
        // schema-building machinery into the shipped binary — around
        // 100 KB, reachable from this one flag — against a recorded
        // binary-size ceiling (D124).
        print!("{}", yunta_core::schema::json(kind));
    } else {
        print!("{}", published(kind));
    }
    Ok(Outcome::Success)
}

/// The catalog, one kind per line with what it is for.
fn list() -> String {
    let mut out = String::from("Documents Yunta reads and validates:\n");
    for kind in ArtifactKind::ALL {
        out.push_str(&format!("  {:<12} {}\n", kind.as_str(), kind.label()));
    }
    out.push_str(
        "\nRun `yunta schema <kind>` for the shape to write, or add `--json` for the \
         JSON Schema an editor can validate against.",
    );
    out
}
