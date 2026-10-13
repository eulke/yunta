//! The menu a paused run offers, rebuilt from its log the way a wake
//! rebuilds it.

use std::path::Path;

/// The menu a paused run rebuilds from its log, read with the trees of every
/// agent CLI, as a wake reads them.
pub async fn rebuilt_menu(
    manifest: &yunta_core::Manifest,
    run_dir: &Path,
    events: &[yunta_core::events::StoredEvent],
) -> Option<(yunta_core::NodeId, yunta_core::events::Escalation)> {
    yunta_engine::current_escalation(manifest, run_dir, &yunta_engine::derive(events))
        .await
        .unwrap()
}
