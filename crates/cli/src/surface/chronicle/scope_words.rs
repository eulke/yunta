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
        Step::Requested { paths } => format!("asks for {}", yunta_core::listed_globs(paths)),
        Step::Granted { by } => format!("scope granted by {}", decider(by)),
        Step::Denied { by, reason } => detailed(
            format!("scope denied by {}", decider(by)),
            &one_line(reason.as_deref().unwrap_or_default()),
        ),
        Step::Withdrawn { paths } => format!(
            "withdrew its request for {}, unanswered because the task closed",
            yunta_core::listed_globs(paths)
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

#[cfg(test)]
mod tests {
    use yunta_core::events::scope::happening::{Happening, Step};
    use yunta_core::{ScopeGlob, TaskId};

    use super::scope_words;

    #[test]
    fn a_withdrawn_request_says_it_was_left_unanswered() {
        let happening = Happening::Expansion {
            task: Some(TaskId::from("task-h")),
            step: Step::Withdrawn {
                paths: vec![ScopeGlob::from("a.txt")],
            },
        };
        assert_eq!(
            scope_words(&happening),
            "task-h withdrew its request for a.txt, unanswered because the task closed"
        );
    }

    #[test]
    fn a_request_names_its_paths_comma_separated() {
        let happening = Happening::Expansion {
            task: Some(TaskId::from("task-h")),
            step: Step::Requested {
                paths: vec![ScopeGlob::from("a.txt"), ScopeGlob::from("src/*.rs")],
            },
        };
        assert_eq!(scope_words(&happening), "task-h asks for a.txt, src/*.rs");
    }
}
