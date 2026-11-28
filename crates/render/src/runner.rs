//! A runner's candidate as a person reads it: the adapter it runs on, and the model after a
//! slash where one is known (`claude-code/opus`), the adapter alone where none is
//! (`claude-code`). A candidate that names no model leaves it to its adapter's CLI, so its
//! model is known once a session reports the one it ran; a sentence that can read the log
//! says that one. Every sentence that names who works a node, a binding `doctor` opens a
//! session on, or a role a setup writes says its candidate here, and the runner a node
//! that names none runs on (`defaults.runner`) is said as the default here too.

use yunta_core::{AdapterId, ModelName, RunnerName};

/// `adapter`, and `model` after a slash where one is known: `claude-code/opus`, or
/// `claude-code` where none is.
pub fn candidate(adapter: &AdapterId, model: Option<&ModelName>) -> String {
    match model {
        Some(model) => format!("{adapter}/{model}"),
        None => adapter.to_string(),
    }
}

/// `runner` and the candidate that works it ([`candidate`]): `planner on claude-code/opus`,
/// or `planner on claude-code` where no model is known.
pub fn on(
    runner: impl std::fmt::Display,
    adapter: &AdapterId,
    model: Option<&ModelName>,
) -> String {
    format!("{runner} on {}", candidate(adapter, model))
}

/// `runner`, said as the default one, which a node that names no runner runs on
/// (`defaults.runner`): `implementer (the default)`.
pub fn the_default(runner: &RunnerName) -> String {
    format!("{runner} (the default)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_candidate_with_no_model_reads_as_its_adapter_alone() {
        let (planner, claude) = (
            RunnerName::from_static("planner"),
            AdapterId::from_static("claude-code"),
        );
        assert_eq!(on(&planner, &claude, None), "planner on claude-code");
        let opus = ModelName::from_static("opus");
        assert_eq!(
            on(&planner, &claude, Some(&opus)),
            "planner on claude-code/opus"
        );
        assert_eq!(candidate(&claude, None), "claude-code");
        assert_eq!(candidate(&claude, Some(&opus)), "claude-code/opus");
    }
}
