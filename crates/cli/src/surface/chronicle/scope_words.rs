//! The words a scope moment is said in: a request to write outside a
//! declared scope and its answer, and what a task may reach beyond it
//! because a shape it owns is named there.

use yunta_core::events::scope;
use yunta_core::text::{detailed, one_line};

pub(super) fn scope_words(happening: &scope::happening::Happening) -> String {
    use scope::happening::{Happening as H, Step};
    let (task, step) = match happening {
        H::Expansion { task, step } => (task, step),
        H::Derived {
            task,
            paths,
            common,
        } => return derived_words(task, paths, common),
    };
    let said = match step {
        Step::Requested { paths } => format!(
            "asks for {}",
            paths
                .iter()
                .map(|glob| glob.as_str().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Step::Granted { by } => format!("scope granted by {}", decider(by)),
        Step::Denied { by, reason } => detailed(
            format!("scope denied by {}", decider(by)),
            &one_line(reason.as_deref().unwrap_or_default()),
        ),
    };
    // A node's own request needs no name in front of it: the moment
    // already carries the node it is written under.
    match task {
        Some(task) => format!("{task} {said}"),
        None => said,
    }
}

/// What a task may write because a shape it owns is named there, and the
/// shapes named too widely to follow.
fn derived_words(
    task: &yunta_core::TaskId,
    paths: &[yunta_core::ScopeGlob],
    common: &[String],
) -> String {
    let reach = match paths {
        [] => format!("{task} reaches no file beyond its scope through its shapes"),
        _ => format!(
            "{task} may also write {}, which name shapes it owns",
            yunta_core::listed_globs(paths)
        ),
    };
    match common {
        [] => reach,
        _ => format!("{reach}; named too widely to follow: {}", common.join(", ")),
    }
}

/// Who settled a scope request.
fn decider(by: &yunta_core::events::Decider) -> String {
    match by {
        yunta_core::events::Decider::Rule => "the rule".to_string(),
        yunta_core::events::Decider::Person { id } => id.to_string(),
        yunta_core::events::Decider::Evidence { criterion } => {
            format!("evidence from `{criterion}`")
        }
    }
}
