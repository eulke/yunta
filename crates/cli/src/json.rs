//! The versioned JSON the CLI emits for machines: `stats --json`,
//! `status --json`, `run --json`, and the same DTOs the `yunta mcp`
//! control plane returns in a tool result. One schema version spans
//! them, so a reader keys off a single number that changes only when a
//! field's meaning does — the CLI's own text output is for people, this
//! is the contract for programs.

use crate::error::{CliError, Outcome};

/// The version stamped on every machine-readable document this CLI emits.
/// Bumped when a field's meaning changes, never for an additive one, so a
/// reader can refuse a document from a schema it predates.
pub const SCHEMA_VERSION: u32 = 1;

/// Serializes a DTO as pretty JSON to stdout — the one place a `--json`
/// command prints its document, reporting a serialization failure as the
/// one error it can hit rather than unwrapping it.
pub fn print_json<T: serde::Serialize>(value: &T) -> Result<Outcome, CliError> {
    println!("{}", to_json_string(value).map_err(CliError::msg)?);
    Ok(Outcome::Success)
}

/// Serializes a DTO as pretty JSON to a string — what the control plane
/// hands back in a tool result instead of printing, so a `println!` never
/// lands in the middle of its stdio JSON-RPC stream.
pub fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| format!("could not serialize output as JSON: {e}"))
}
