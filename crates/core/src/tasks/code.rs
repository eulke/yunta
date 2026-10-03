//! What a change's code has to show a person reviewing a plan: each
//! symbol its `at` names, and code rather than a sentence about it. A
//! reviewer decides on how the work will look, and a signature left out
//! or a comment in its place is a decision taken on nothing.

use super::proof::{as_a_word, documentation};
use super::rules::broke;
use crate::diagnostic::{Diagnostic, RuleCode};
use crate::{Change, Shape, Task};

/// Each change of `task` whose code leaves out a symbol its `at` names —
/// shown neither by the code nor by a shape the task declares in that
/// file — or says only a comment.
pub(super) fn unshown(index: usize, task: &Task, shapes: &[Shape]) -> Vec<Diagnostic> {
    let mut broken = Vec::new();
    for change in &task.changes {
        let Some(code) = change
            .code
            .as_deref()
            .filter(|code| !code.trim().is_empty())
        else {
            continue;
        };
        let shaped: Vec<&str> = shapes
            .iter()
            .filter(|shape| shape.owner == task.id && shape.file == change.file())
            .map(|shape| shape.code.as_str())
            .collect();
        if let Some(missing) = missing(change, code, &shaped) {
            broken.push(broke(
                index,
                &task.id,
                RuleCode::ChangeCodeMissesAName,
                missing,
            ));
        }
        if !documentation(change.file()) && only_comments(code) {
            broken.push(broke(
                index,
                &task.id,
                RuleCode::ChangeCodeIsAComment,
                format!(
                    "the change at `{}` has only a comment for its `code`; give the code it adds \
                     or changes — a comment says what the code will do, and a person decides on \
                     how it looks",
                    change.at
                ),
            ));
        }
    }
    broken
}

/// What `change` says it changes and neither its `code` nor a shape in
/// its file shows, said as what to give; `None` when every name shows.
fn missing(change: &Change, code: &str, shaped: &[&str]) -> Option<String> {
    let missing: Vec<String> = named(change)
        .into_iter()
        .filter(|name| !as_a_word(code, name) && !shaped.iter().any(|shape| as_a_word(shape, name)))
        .map(|name| format!("`{name}`"))
        .collect();
    (!missing.is_empty()).then(|| {
        format!(
            "the change at `{}` names {}, and its `code` does not show {}; give the declaration \
             of each — its whole signature, or the type with the fields it adds",
            change.at,
            missing.join(", "),
            if missing.len() == 1 { "it" } else { "them" }
        )
    })
}

/// The symbols `change.at` names after its file: `pack.rs::add/update`
/// names `add` and `update`, `greet.rs::Greeter::bye` names `bye`. A
/// place said in words names none.
fn named(change: &Change) -> Vec<&str> {
    let Some((_, item)) = change.at.split_once("::") else {
        return Vec::new();
    };
    let last = item.rsplit("::").next().unwrap_or(item);
    last.split(['/', ','])
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect()
}

/// Whether every line `code` says is a comment.
fn only_comments(code: &str) -> bool {
    let mut lines = code
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .peekable();
    lines.peek().is_some() && lines.all(comment)
}

/// Whether `line` is a comment in the languages a plan writes code in:
/// `//`, `/*` and its `*` lines, `<!--`, `--`, and `#` where it is not an
/// attribute or a shebang. A doc comment (`///`, `//!`, `/**`, `/*!`) is
/// what the file will say, not a note about it, and is not one.
fn comment(line: &str) -> bool {
    let documents = ["///", "//!", "/**", "/*!"]
        .iter()
        .any(|marker| line.starts_with(marker));
    let marked = ["//", "/*", "*", "<!--", "--"]
        .iter()
        .any(|marker| line.starts_with(marker));
    (marked && !documents)
        || (line.starts_with('#') && !line.starts_with("#[") && !line.starts_with("#!"))
}
