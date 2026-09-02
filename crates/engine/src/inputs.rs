//! Resolving `inputs:`: turning what the CLI
//! was handed on `--input k=v` plus each declared input's own `default`
//! into the frozen, per-name string map the manifest carries and
//! `{{inputs.*}}` templates read from. Everything here runs once, before
//! the run's worktree or first token exist — everything is validated
//! before the first token — a bad input is meant to be the cheapest
//! possible failure, not the latest.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use thiserror::Error;
use yunta_core::InputSpec;

#[derive(Debug, Error, PartialEq)]
pub enum InputsError {
    /// Every `--input` name the workflow does not declare, sorted, so
    /// one run of the command reports them all.
    #[error(
        "unknown input{} `{}` — this workflow declares {}",
        if names.len() > 1 { "s" } else { "" },
        names.join("`, `"),
        if declared.is_empty() { "no inputs".to_string() } else { format!("only: {}", declared.join(", ")) }
    )]
    Unknown {
        names: Vec<String>,
        declared: Vec<String>,
    },

    #[error("input `{name}` is required and has no default — pass `--input {name}=...`")]
    Missing { name: String },

    #[error("input `{name}` expects a finite number, got `{value}`")]
    InvalidNumber { name: String, value: String },

    #[error("input `{name}` expects `true` or `false`, got `{value}`")]
    InvalidBoolean { name: String, value: String },

    #[error("input `{name}` must be >= {min}, got {value}")]
    BelowMin { name: String, value: f64, min: f64 },

    #[error("input `{name}` must be <= {max}, got {value}")]
    AboveMax { name: String, value: f64, max: f64 },

    #[error("input `{name}` must be at least {min_length} characters, got {actual}")]
    TooShort {
        name: String,
        min_length: u32,
        actual: usize,
    },

    #[error("input `{name}` must match pattern `{pattern}`, got `{value}`")]
    PatternMismatch {
        name: String,
        pattern: String,
        value: String,
    },

    #[error("input `{name}` must be one of [{}], got `{value}`", values.join(", "))]
    NotInEnum {
        name: String,
        value: String,
        values: Vec<String>,
    },

    #[error("input `{name}` names a path that doesn't exist: `{path}`")]
    PathNotFound { name: String, path: String },

    /// Reachable only if a bad `pattern:` slipped past `check` (which
    /// validates every pattern compiles) — resolution still
    /// reports it as a typed error rather than panicking on attacker- or
    /// author-supplied regex it never got to see statically.
    #[error("input `{name}`'s pattern `{pattern}` is not a valid regex: {detail}")]
    InvalidPattern {
        name: String,
        pattern: String,
        detail: String,
    },
}

/// Resolves every declared input to its final string value: `provided`
/// wins when given, the spec's own `default` otherwise, and every value
/// (from either source) is validated against its type before it reaches
/// the manifest. `base_dir` is where a `path`-typed input's existence
/// check resolves a relative path against — the run's original checkout,
/// since this runs before any worktree exists (isolation is a
/// property of the run, not of resolving its inputs).
pub fn resolve_inputs(
    specs: &BTreeMap<String, InputSpec>,
    provided: &HashMap<String, String>,
    base_dir: &Path,
) -> Result<BTreeMap<String, String>, InputsError> {
    let mut unknown: Vec<String> = provided
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

    let mut resolved = BTreeMap::new();
    for (name, spec) in specs {
        let raw = match provided.get(name) {
            Some(value) => value.clone(),
            None => match default_as_string(spec) {
                Some(default) => default,
                None => return Err(InputsError::Missing { name: name.clone() }),
            },
        };
        resolved.insert(name.clone(), validate(name, spec, &raw, base_dir)?);
    }
    Ok(resolved)
}

fn default_as_string(spec: &InputSpec) -> Option<String> {
    match spec {
        InputSpec::String { default, .. } => default.clone(),
        InputSpec::Number { default, .. } => default.map(format_number),
        InputSpec::Boolean { default, .. } => default.map(|b| b.to_string()),
        InputSpec::Enum { default, .. } => default.clone(),
        InputSpec::Path { default, .. } => default.clone(),
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

fn validate(
    name: &str,
    spec: &InputSpec,
    raw: &str,
    base_dir: &Path,
) -> Result<String, InputsError> {
    match spec {
        InputSpec::String {
            pattern,
            min_length,
            ..
        } => {
            if let Some(min_length) = min_length {
                if raw.chars().count() < *min_length as usize {
                    return Err(InputsError::TooShort {
                        name: name.to_string(),
                        min_length: *min_length,
                        actual: raw.chars().count(),
                    });
                }
            }
            if let Some(pattern) = pattern {
                let re = regex::Regex::new(pattern).map_err(|e| InputsError::InvalidPattern {
                    name: name.to_string(),
                    pattern: pattern.clone(),
                    detail: e.to_string(),
                })?;
                if !re.is_match(raw) {
                    return Err(InputsError::PatternMismatch {
                        name: name.to_string(),
                        pattern: pattern.clone(),
                        value: raw.to_string(),
                    });
                }
            }
            Ok(raw.to_string())
        }
        InputSpec::Number { min, max, .. } => {
            let value: f64 = raw
                .parse()
                .ok()
                .filter(|number: &f64| number.is_finite())
                .ok_or_else(|| InputsError::InvalidNumber {
                    name: name.to_string(),
                    value: raw.to_string(),
                })?;
            if let Some(min) = min {
                if value < *min {
                    return Err(InputsError::BelowMin {
                        name: name.to_string(),
                        value,
                        min: *min,
                    });
                }
            }
            if let Some(max) = max {
                if value > *max {
                    return Err(InputsError::AboveMax {
                        name: name.to_string(),
                        value,
                        max: *max,
                    });
                }
            }
            Ok(format_number(value))
        }
        InputSpec::Boolean { .. } => match raw {
            "true" | "false" => Ok(raw.to_string()),
            other => Err(InputsError::InvalidBoolean {
                name: name.to_string(),
                value: other.to_string(),
            }),
        },
        InputSpec::Enum { values, .. } => {
            if values.iter().any(|v| v == raw) {
                Ok(raw.to_string())
            } else {
                Err(InputsError::NotInEnum {
                    name: name.to_string(),
                    value: raw.to_string(),
                    values: values.clone(),
                })
            }
        }
        InputSpec::Path { .. } => {
            let path = base_dir.join(raw);
            if path.exists() {
                Ok(raw.to_string())
            } else {
                Err(InputsError::PathNotFound {
                    name: name.to_string(),
                    path: raw.to_string(),
                })
            }
        }
    }
}
