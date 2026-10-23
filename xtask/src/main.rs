//! Repository tooling, run as `cargo xtask <command>`. Its map:
//!
//! - `schema`, in this file, writes the JSON Schema of every authored document
//!   and of one event of the log under `crates/core/schemas/`, generated from
//!   the types that read them; `schema --check` verifies the committed files
//!   are what the types emit.
//! - `smells` holds the smell counters: it lowers the ratchet's counts in
//!   `xtask/smells.baseline` to what the tree counts and refuses a rise;
//!   `smells --check` fails when a count rises above that baseline or the merge
//!   base's.
//! - `docs` is the docs checker: `docs --check` holds the agent docs and the map
//!   to their shapes, and `docs --about <paths>` prints what they record about
//!   those paths.
//! - `adr` derives the index of the decision records from the decision files
//!   and proves what they say about each other; `adr --check` fails when the two
//!   disagree.
//!
//! All four checks run in CI.

mod adr;
mod docs;
mod smells;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use yunta_core::durable::{self, Durability};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["schema"] => schema(Mode::Write),
        ["schema", "--check"] => schema(Mode::Check),
        ["smells"] => smells::write(),
        ["smells", "--check"] => smells::check(),
        ["adr"] => adr::run(Mode::Write),
        ["adr", "--check"] => adr::run(Mode::Check),
        ["docs", "--check"] => docs::check(),
        ["docs", "--about", paths @ ..] => docs::about(
            &paths
                .iter()
                .map(|path| path.to_string())
                .collect::<Vec<_>>(),
        ),
        _ => Err(
            "usage: cargo xtask <schema|smells|adr> [--check] | docs <--check|--about [<paths>]>"
                .to_string(),
        ),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Write,
    Check,
}

/// Where the schema files live: `crates/core/schemas/`.
///
/// Inside the package that declares the types, so `yunta-core` embeds
/// them with `include_str!` and every crate that ships a schema reads
/// the same bytes CI checked. A directory at the workspace root would
/// be outside every package, and a build that reached into it would
/// stop working the day the workspace is published.
fn schemas_dir() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("crates").join("core").join("schemas"))
        .ok_or_else(|| "the xtask crate sits directly under the workspace root".to_string())
}

fn render(schema: &yunta_core::schema::Schema) -> Result<String, String> {
    serde_json::to_string_pretty(schema)
        .map(|json| json + "\n")
        .map_err(|error| format!("cannot render a schema as JSON: {error}"))
}

fn schema(mode: Mode) -> Result<(), String> {
    let dir = schemas_dir()?;
    let mut stale = Vec::new();
    for (name, schema) in yunta_core::schema::all() {
        let path = dir.join(format!("{name}.json"));
        let rendered = render(&schema)?;
        match mode {
            Mode::Write => {
                std::fs::create_dir_all(&dir)
                    .map_err(|error| format!("cannot create `{}`: {error}", dir.display()))?;
                durable::replace(&path, rendered.as_bytes(), Durability::Atomic)
                    .map_err(|error| format!("cannot write `{}`: {error}", path.display()))?;
                println!("wrote {}", path.display());
            }
            Mode::Check => {
                let current = std::fs::read_to_string(&path).unwrap_or_default();
                if current != rendered {
                    stale.push(path);
                }
            }
        }
    }
    if stale.is_empty() {
        return Ok(());
    }
    Err(format!(
        "these schema files differ from what the types emit — run `cargo xtask schema` and \
         commit the result:\n{}",
        stale
            .iter()
            .map(|path| format!("  {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}
