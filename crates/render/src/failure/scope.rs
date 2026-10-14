//! The words of a node's scope: the files its work wrote outside the globs
//! it may write, a session that asked for more scope, a loop's requests a
//! person owes an answer to, and the paths the project protects.

use std::path::PathBuf;

use yunta_core::events::{Failure, OwedScope, RequestedScope};
use yunta_core::protected::Protection;
use yunta_core::{listed_globs, text};

/// Paths as a sentence lists them: comma-separated, each as its display.
pub(crate) fn paths_listed(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Work that reached paths outside its scope, named one by one: how many
/// files, and which.
pub(super) fn violated(outside_scope: &[PathBuf]) -> String {
    format!(
        "scope violated: {} outside the declared globs — {}",
        text::counted(outside_scope.len(), "file"),
        paths_listed(outside_scope)
    )
}

/// The headline of a scope violation over several files: how many fell outside.
pub(super) fn violated_headline(outside_scope: &[PathBuf]) -> String {
    format!(
        "scope violated: {} outside the declared globs",
        text::counted(outside_scope.len(), "file")
    )
}

/// Each path that fell outside a scope, a line each.
pub(super) fn violated_detail(outside_scope: &[PathBuf]) -> Vec<String> {
    outside_scope
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

/// A session that asked for scope beyond its own: the globs and the reason.
pub(super) fn requested(requested_scope: &RequestedScope) -> String {
    format!(
        "asked for scope beyond its own — {}: {}",
        listed_globs(&requested_scope.paths),
        requested_scope.reason
    )
}

/// The requests a loop owes a person, as one sentence.
pub(super) fn owed(owed: &[OwedScope]) -> String {
    let each: Vec<String> = owed
        .iter()
        .map(|owed| {
            format!(
                "task `{}` asked for {}",
                owed.task_id,
                listed_globs(&owed.paths)
            )
        })
        .collect();
    format!(
        "{} a person's decision about scope: {}",
        text::counted_as(owed.len(), "request owes", "requests owe"),
        each.join("; ")
    )
}

/// One request a loop owes a person: the task's paths, and its reason.
pub(super) fn owed_line(owed: &OwedScope) -> String {
    format!(
        "`{}` — {}: {}",
        owed.task_id,
        listed_globs(&owed.paths),
        owed.reason
    )
}

/// The paths a denial names, each with the reason it is protected when the
/// log says one: the reasons line up with the paths by index.
pub(super) fn denied(failure: &Failure) -> String {
    let Failure::PathsDenied {
        denied_paths: paths,
        reasons,
    } = failure
    else {
        unreachable!("routed here only for a denied path")
    };
    let listed: Vec<String> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| match reasons.get(index) {
            Some(reason) => format!("{} ({})", path.display(), protection_words(*reason)),
            None => path.display().to_string(),
        })
        .collect();
    format!(
        "wrote what no session of the run may write — {}: undo those changes",
        listed.join(", ")
    )
}

/// The words a protected path's reason reads as, after the path it names.
pub(crate) fn protection_words(protection: Protection) -> &'static str {
    match protection {
        Protection::ProjectDeny => "denied by the project's permissions",
        Protection::AgentConfig => "an agent CLI's own configuration",
        Protection::AgentInstructions => {
            "an instruction file: only a person grants it, and only to a session that asks"
        }
        Protection::ApprovedTest => "an approved test",
        Protection::GitRegister => "the run's git record",
    }
}
