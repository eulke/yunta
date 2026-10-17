//! The tasks whose tests are owed again: a person accepted that a test
//! the run's spec gave them is wrong, and the node that writes the spec
//! has not handed one over since.

use yunta_core::events::{ArtifactId, Respecified};
use yunta_core::{ArtifactKind, TaskId};

use crate::replay::RunState;

/// Every task whose tests a person accepted are wrong and that the node
/// writing them has not written again since: the node accepted no spec
/// after the person answered.
pub(crate) fn respecifications_owed(state: &RunState) -> Vec<(&TaskId, &Respecified)> {
    let spec = ArtifactId::Interpreted {
        kind: ArtifactKind::Spec,
    };
    state
        .tasks
        .iter()
        .filter_map(|(task, record)| record.respecified.as_ref().map(|owed| (task, owed)))
        .filter(|(_, owed)| {
            state
                .artifacts
                .latest(&spec, Some(&owed.by))
                .is_none_or(|written| written.seq < owed.at)
        })
        .collect()
}
