//! The run's findings as a gate shows them: one view of every finding
//! standing in the run — whoever reported it, the engine's own included —
//! with the answers other nodes gave it, pinned to the gate's question by
//! the hash of its bytes.
//!
//! A view rather than an artifact: no node produces it, and it derives
//! from the log alone, so a process that never asked the question — a
//! `resolve_gate` from the control plane — rebuilds the identical one.

use yunta_core::events::{ArtifactId, Shown};
use yunta_core::{ArtifactContextRef, ArtifactKind, ContentHash};

use super::{RunCtx, RunError};
use crate::replay::RunState;

/// The identity the view is shown under.
fn findings() -> ArtifactId {
    ArtifactId::Interpreted {
        kind: ArtifactKind::Findings,
    }
}

/// Whether `reference` asks for the run's findings: a `findings` read
/// that names no node.
pub(crate) fn is_view(reference: &ArtifactContextRef) -> bool {
    reference.node.is_none() && ArtifactId::from(&reference.id) == findings()
}

/// Whether `shown` is the view, rather than a node's findings document.
pub(crate) fn shows_view(shown: &Shown) -> bool {
    shown.producer.is_none() && shown.artifact == findings()
}

/// The view's bytes, as the run's log stands in `state`.
pub(crate) fn view_bytes(state: &RunState) -> Result<Vec<u8>, yunta_core::yaml::YamlError> {
    yunta_core::yaml::to_string(&state.findings.standing()).map(String::into_bytes)
}

/// What a gate records it shows when it shows the view.
pub(crate) fn shown(state: &RunState) -> Result<Shown, yunta_core::yaml::YamlError> {
    Ok(Shown {
        producer: None,
        artifact: findings(),
        content_hash: ContentHash::sha256(&view_bytes(state)?),
    })
}

/// Keeps the view's bytes in the run's object store, where what a gate
/// shows is read back by its hash, and writes the file a person opens.
pub(crate) async fn keep(ctx: &RunCtx<'_>, state: &RunState) -> Result<(), RunError> {
    let bytes = view_bytes(state).map_err(|source| RunError::Broken {
        diagnostic: format!("the run's findings could not be written: {source}"),
    })?;
    let io = |action: &str, source: std::io::Error| RunError::Broken {
        diagnostic: format!("the run's findings could not be {action}: {source}"),
    };
    let hash = crate::artifacts::store::ObjectStore::at(ctx.run_dir)
        .put(&bytes)
        .await
        .map_err(|source| io("stored", source))?;
    let shown = Shown {
        producer: None,
        artifact: findings(),
        content_hash: hash,
    };
    let path = ctx.run_dir.join(crate::artifacts::shown::view_of(&shown));
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| io("written", source))?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|source| io("written", source))
}
