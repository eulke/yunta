//! The words of an input that does not resolve, as a reader sees them: the
//! name it was given, what it said, and what the workflow declares. The
//! engine names each problem by its kind and its fields; the words are here.

use yunta_core::inputs::InputsProblem;
use yunta_core::{text, InputName};

/// The problem an input has, in the words a reader sees.
pub fn problem(problem: &InputsProblem) -> String {
    match problem {
        InputsProblem::Unknown { names, declared } => unknown(names, declared),
        InputsProblem::Missing { name } => format!("input `{name}` is required and has no default"),
        InputsProblem::PathNotFound { name, path } => {
            format!("input `{name}` names a path that doesn't exist: `{path}`")
        }
        InputsProblem::DocumentUnreadable {
            name,
            path,
            label,
            detail,
        } => format!(
            "input `{name}` names `{path}`, which cannot be read as a file: {detail} — point the \
             input at the {label} itself"
        ),
        InputsProblem::DocumentRefused { name, report } => format!(
            "input `{name}` is not a {} this run can hold\n{report}",
            report.document.label()
        ),
        InputsProblem::InvalidPattern {
            name,
            pattern,
            detail,
        } => format!("input `{name}`'s pattern `{pattern}` is not a valid regex: {detail}"),
        InputsProblem::InvalidNumber { .. }
        | InputsProblem::InvalidBoolean { .. }
        | InputsProblem::BelowMin { .. }
        | InputsProblem::AboveMax { .. }
        | InputsProblem::TooShort { .. }
        | InputsProblem::PatternMismatch { .. }
        | InputsProblem::NotInEnum { .. } => value(problem),
    }
}

/// The inputs a run was handed that the workflow does not declare, and what
/// it does declare.
fn unknown(names: &[InputName], declared: &[InputName]) -> String {
    let declares = match declared.is_empty() {
        true => "no inputs".to_string(),
        false => format!("only: {}", listed(declared, ", ")),
    };
    format!(
        "unknown {} `{}` — this workflow declares {declares}",
        text::agreeing(names.len(), "input", "inputs"),
        listed(names, "`, `"),
    )
}

/// A value that does not fit the input it was given to: its kind, its bounds,
/// its pattern or its choices.
fn value(problem: &InputsProblem) -> String {
    match problem {
        InputsProblem::InvalidNumber { name, value } => {
            format!("input `{name}` expects a finite number, got `{value}`")
        }
        InputsProblem::InvalidBoolean { name, value } => {
            format!("input `{name}` expects `true` or `false`, got `{value}`")
        }
        InputsProblem::BelowMin { name, value, min } => {
            format!("input `{name}` must be >= {min}, got {value}")
        }
        InputsProblem::AboveMax { name, value, max } => {
            format!("input `{name}` must be <= {max}, got {value}")
        }
        InputsProblem::TooShort {
            name,
            min_length,
            actual,
        } => format!("input `{name}` must be at least {min_length} characters, got {actual}"),
        InputsProblem::PatternMismatch {
            name,
            pattern,
            value,
        } => format!("input `{name}` must match pattern `{pattern}`, got `{value}`"),
        InputsProblem::NotInEnum {
            name,
            value,
            values,
        } => format!(
            "input `{name}` must be one of [{}], got `{value}`",
            values.join(", ")
        ),
        _ => unreachable!("routed here only for a value that does not fit its input"),
    }
}

/// Names joined by `separator`, each as its text.
fn listed(names: &[InputName], separator: &str) -> String {
    names
        .iter()
        .map(InputName::as_str)
        .collect::<Vec<_>>()
        .join(separator)
}
