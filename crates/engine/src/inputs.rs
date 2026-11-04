//! Resolving `inputs:`: turning what the CLI
//! was handed on `--input k=v` plus each declared input's own `default`
//! into the frozen, per-name string map the manifest carries and
//! `{{inputs.*}}` templates read from. Everything here runs once, before
//! the run's worktree or its first token exist: a bad input is meant to be
//! the cheapest possible failure, not the latest. A default is resolved
//! here and frozen with the rest, never by a node, and a `path` must exist
//! now, whether it names a file or a directory — a name for something not
//! there yet is a `string`.
//!
//! A `document` input resolves further: the file is read as the kind it
//! declares, through the one door that runs the kind's shape and rules
//! together, and becomes an artifact the run is born holding. The run
//! therefore gets its tasks document from whoever started it, with no
//! node standing in to copy a file the engine could read itself. Its
//! frozen value is the hash of the document the run holds,
//! `sha256:<hash>`, not the path it was read from, so the run outlives the
//! file.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use yunta_core::events::ArtifactId;
use yunta_core::{ArtifactKind, InputName, InputSpec};

use crate::run::{BirthArtifact, BirthOrigin};

/// Why the inputs a run was handed do not resolve to its workflow's declared
/// inputs: the core's account of each problem, whose display is a code.
pub use yunta_core::inputs::InputsProblem as InputsError;

/// What a workflow's `inputs:` resolve to: the value of every declared
/// input, and the documents that entered as one.
///
/// Two returns rather than a map of two kinds of thing, because they
/// travel to two different places and neither can stand in for the
/// other: `values` is frozen into the manifest and is what
/// `{{inputs.*}}` renders, while `documents` are bytes that go on the
/// run's log at birth and are never part of its manifest.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedInputs {
    /// Every declared input by name, as the manifest freezes it.
    pub values: BTreeMap<InputName, String>,
    /// One artifact per `document` input, in declaration order: what the
    /// run holds from birth, before any node runs.
    pub documents: Vec<BirthArtifact>,
}

/// Resolves every declared input to its final string value: `provided`
/// wins when given, the spec's own `default` otherwise, and every value
/// (from either source) is validated against its type before it reaches
/// the manifest. A `document` input is read and validated here too, and
/// its value is the identity of the document that was read. `base_dir`
/// is where a `path`- or `document`-typed input resolves a relative path
/// against — the run's original checkout, since this runs before any
/// worktree exists (isolation is a property of the run, not of resolving
/// its inputs).
pub async fn resolve_inputs(
    specs: &BTreeMap<InputName, InputSpec>,
    provided: &HashMap<InputName, String>,
    base_dir: &Path,
) -> Result<ResolvedInputs, InputsError> {
    let mut unknown: Vec<InputName> = provided
        .keys()
        .filter(|name| !specs.contains_key(*name))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        unknown.sort();
        return Err(InputsError::Unknown {
            names: unknown,
            declared: specs.keys().cloned().collect(),
        });
    }

    let mut resolved = ResolvedInputs::default();
    for (name, spec) in specs {
        let raw = match provided.get(name) {
            Some(value) => value.clone(),
            None => match default_as_string(spec) {
                Some(default) => default,
                None => return Err(InputsError::Missing { name: name.clone() }),
            },
        };
        let value = match validate(name, spec, &raw, base_dir).await? {
            Resolved::Value(value) => value,
            Resolved::Document(document) => {
                // The frozen value is the document itself, not the file
                // it came from: the run outlives that path, and whoever
                // opened it again could be reading something else.
                let value = yunta_core::sha256_hex(&document.bytes).qualified();
                resolved.documents.push(document);
                value
            }
        };
        resolved.values.insert(name.clone(), value);
    }
    Ok(resolved)
}

/// What resolving one declared input yields: a value, or the document
/// the run is born holding — whose value is then its own identity.
enum Resolved {
    Value(String),
    Document(BirthArtifact),
}

/// The document one `document` input names, as the run will hold it.
///
/// Existence, readability and content are three separate answers on
/// purpose: the first two are about the file a person typed, the third
/// about what they wrote in it, and only the third is a report.
async fn read_document(
    name: &InputName,
    kind: ArtifactKind,
    raw: &str,
    base_dir: &Path,
) -> Result<BirthArtifact, InputsError> {
    let path = base_dir.join(raw);
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(InputsError::PathNotFound {
            name: name.clone(),
            path: raw.to_string(),
        });
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|source| InputsError::DocumentUnreadable {
            name: name.clone(),
            path: raw.to_string(),
            label: kind.label().to_string(),
            detail: source.to_string(),
        })?;
    let canonical = crate::artifacts::canonical_document(kind, &bytes, raw).map_err(|report| {
        InputsError::DocumentRefused {
            name: name.clone(),
            report,
        }
    })?;
    Ok(BirthArtifact {
        artifact: ArtifactId::Interpreted { kind },
        origin: BirthOrigin::Input {
            input: name.clone(),
        },
        bytes: canonical,
    })
}

fn default_as_string(spec: &InputSpec) -> Option<String> {
    match spec {
        InputSpec::String { default, .. } => default.clone(),
        InputSpec::Number { default, .. } => default.map(format_number),
        InputSpec::Boolean { default, .. } => default.map(|b| b.to_string()),
        InputSpec::Enum { default, .. } => default.clone(),
        InputSpec::Path { default, .. } => default.clone(),
        InputSpec::Document { default, .. } => default.clone(),
    }
}

/// Integers render without a trailing `.0` — `max_tasks: 40` should read
/// back as `40` everywhere it's templated, not `40.0`.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.is_finite() {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

/// One input's raw text held to everything its type demands.
async fn validate(
    name: &InputName,
    spec: &InputSpec,
    raw: &str,
    base_dir: &Path,
) -> Result<Resolved, InputsError> {
    let value = match spec {
        InputSpec::String {
            pattern,
            min_length,
            ..
        } => validate_string(name, raw, pattern.as_deref(), *min_length)?,
        InputSpec::Number { min, max, .. } => validate_number(name, raw, *min, *max)?,
        InputSpec::Boolean { .. } => match raw {
            "true" | "false" => raw.to_string(),
            other => {
                return Err(InputsError::InvalidBoolean {
                    name: name.clone(),
                    value: other.to_string(),
                })
            }
        },
        InputSpec::Enum { values, .. } => {
            if !values.iter().any(|v| v == raw) {
                return Err(InputsError::NotInEnum {
                    name: name.clone(),
                    value: raw.to_string(),
                    values: values.clone(),
                });
            }
            raw.to_string()
        }
        // What a path is checked for is that it exists: a missing file
        // fails the run either way, and failing before the first token
        // is the cheap place to do it.
        InputSpec::Path { .. } => {
            if !base_dir.join(raw).exists() {
                return Err(InputsError::PathNotFound {
                    name: name.clone(),
                    path: raw.to_string(),
                });
            }
            raw.to_string()
        }
        // A document is held to its kind, not to its path: what the run
        // takes from it is the document, and the value follows from
        // that.
        InputSpec::Document { kind, .. } => {
            return read_document(name, *kind, raw, base_dir)
                .await
                .map(Resolved::Document)
        }
    };
    Ok(Resolved::Value(value))
}

fn validate_string(
    name: &InputName,
    raw: &str,
    pattern: Option<&str>,
    min_length: Option<u32>,
) -> Result<String, InputsError> {
    if let Some(min_length) = min_length {
        if raw.chars().count() < min_length as usize {
            return Err(InputsError::TooShort {
                name: name.clone(),
                min_length,
                actual: raw.chars().count(),
            });
        }
    }
    if let Some(pattern) = pattern {
        let re = regex::Regex::new(pattern).map_err(|e| InputsError::InvalidPattern {
            name: name.clone(),
            pattern: pattern.to_string(),
            detail: e.to_string(),
        })?;
        if !re.is_match(raw) {
            return Err(InputsError::PatternMismatch {
                name: name.clone(),
                pattern: pattern.to_string(),
                value: raw.to_string(),
            });
        }
    }
    Ok(raw.to_string())
}

fn validate_number(
    name: &InputName,
    raw: &str,
    min: Option<f64>,
    max: Option<f64>,
) -> Result<String, InputsError> {
    let value: f64 = raw
        .parse()
        .ok()
        .filter(|number: &f64| number.is_finite())
        .ok_or_else(|| InputsError::InvalidNumber {
            name: name.clone(),
            value: raw.to_string(),
        })?;
    if let Some(min) = min.filter(|min| value < *min) {
        return Err(InputsError::BelowMin {
            name: name.clone(),
            value,
            min,
        });
    }
    if let Some(max) = max.filter(|max| value > *max) {
        return Err(InputsError::AboveMax {
            name: name.clone(),
            value,
            max,
        });
    }
    Ok(format_number(value))
}
