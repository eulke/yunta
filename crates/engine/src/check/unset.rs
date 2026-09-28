//! See [`super`]. The config keys each node cannot run without.

use super::*;
use yunta_core::ConfigKey;

/// Every node whose kind cannot run with a key the config leaves unset:
/// a coverage gate with no `coverage`, an executor nobody registered, a
/// session with no runner to open on.
pub(crate) fn check_unset_keys(
    workflow: &Workflow,
    config: &ConfigLayer,
    errors: &mut Vec<CheckError>,
) {
    for node in workflow.iter_nodes() {
        if let Some(key) = ConfigKey::unset(node, config) {
            errors.push(CheckError::Unset {
                node: node.id.clone(),
                key,
            });
        }
    }
}
