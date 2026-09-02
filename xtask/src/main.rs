//! Repository tooling, run as `cargo xtask <command>`.
//!
//! `schema` writes the JSON Schema of every authored document and of
//! one event of the log under `schemas/`, generated from the types that
//! read them; `schema --check` verifies the committed files are exactly
//! what the types emit, which is what CI runs.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
        _ => Err("usage: cargo xtask schema [--check]".to_string()),
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
enum Mode {
    Write,
    Check,
}

/// Where the schema files live: `schemas/` at the workspace root.
fn schemas_dir() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("schemas"))
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
                std::fs::write(&path, &rendered)
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
