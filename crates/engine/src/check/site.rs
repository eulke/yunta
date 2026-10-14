//! Where a node is declared in a workflow file.

use yunta_core::yaml::Pointer;
use yunta_core::NodeId;

/// Where the node `id` is declared in a workflow file.
pub(crate) fn node(id: &NodeId) -> Pointer {
    Pointer::root().key("nodes").node(id.as_str())
}
