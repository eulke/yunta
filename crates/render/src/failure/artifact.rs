//! The words of artifacts: the documents that did not close, the bytes a
//! run cannot hand over, a name the engine refuses, a mount it cannot reach,
//! a template that does not render, and the artifact an external gate
//! publishes that no node of the run produced.

use yunta_core::events::{ArtifactId, MountError, NameProblem, ObjectProblem};
use yunta_core::template::TemplateError;
use yunta_core::{ArtifactFailure, InputName, NodeId};

/// Each document that did not close, one block after another.
pub(super) fn artifacts_said(artifacts: &[ArtifactFailure]) -> String {
    artifacts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each document that did not close, by the heading its own block opens
/// with: the path and how many problems it has.
pub(super) fn headline(artifacts: &[ArtifactFailure]) -> String {
    artifacts
        .iter()
        .filter_map(|artifact| artifact.to_string().lines().next().map(str::to_string))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The problems of each document that did not close: each block's own
/// lines, under its heading when there is one document to name.
pub(super) fn detail(artifacts: &[ArtifactFailure]) -> Vec<String> {
    let blocks = artifacts.iter().map(ToString::to_string);
    match artifacts.len() {
        1 => blocks.flat_map(|block| own_lines(&block, 1)).collect(),
        _ => blocks.flat_map(|block| own_lines(&block, 0)).collect(),
    }
}

/// The lines of `block` after its first `skip`, without their indent.
fn own_lines(block: &str, skip: usize) -> Vec<String> {
    block
        .lines()
        .skip(skip)
        .map(|line| line.trim().to_string())
        .collect()
}

/// The bytes an artifact names, which the run cannot hand over, and why.
pub(super) fn bytes(artifact: &ArtifactId, problem: &ObjectProblem) -> String {
    format!(
        "the {}: the child run cannot hand over its bytes: {}",
        artifact.label(),
        object(problem)
    )
}

/// Why a run cannot hand over the bytes an artifact names.
pub fn object(problem: &ObjectProblem) -> String {
    problem.sentence()
}

/// Why an artifact name cannot be taken, and the name as the run wrote it.
pub fn name(written: &str, problem: &NameProblem) -> String {
    let said: String = match problem {
        NameProblem::Empty => "an artifact is named by a file name, and this one is empty".into(),
        NameProblem::OutsideRun => format!(
            "`{written}` reaches outside the run directory — an artifact name is relative, with \
             no `..` component"
        ),
        NameProblem::Reserved { identity } => format!(
            "`{written}` is how the run's view names {identity}, so an artifact cannot take it"
        ),
    };
    format!("does not parse: {said}")
}

/// Why a mount the run cannot reach says so, for the artifact and its node.
pub fn mount(error: &MountError) -> String {
    match error {
        MountError::NoTerminalChild { artifact, node } => format!(
            "mount of the {artifact} from node `{node}`: no linked child run of `{node}` reached \
             a terminal state in this run — did this run's mode exclude it?"
        ),
        MountError::Unreadable {
            artifact,
            node,
            run,
            ..
        } => format!(
            "mount of the {artifact} from node `{node}`: run `{run}` cannot hand over its bytes"
        ),
        MountError::Unfrozen {
            artifact,
            node,
            run,
            detail,
        } => format!(
            "mount of the {artifact} from node `{node}`: the frozen truth of run `{run}` cannot be \
             read: {detail}"
        ),
        MountError::Unlogged {
            artifact,
            node,
            run,
            ..
        } => format!(
            "mount of the {artifact} from node `{node}`: the log of run `{run}` cannot be reached"
        ),
    }
}

/// A template that does not render: the template's own account, and under a
/// child workflow's input, the input it was rendering.
pub(super) fn template(template: &TemplateError, input: &Option<InputName>) -> String {
    match input {
        Some(input) => format!("child input `{input}` does not render: {template}"),
        None => template.to_string(),
    }
}

/// An external gate publishes an artifact that no node of this run produced.
pub(super) fn gate_missing(node: &NodeId, artifact: &ArtifactId) -> String {
    format!(
        "node `{node}` publishes the {} with its external gate, and this run holds no such \
         artifact — no node produced it",
        artifact.label()
    )
}
