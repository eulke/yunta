//! One declared artifact of `on_finish.distill`, copied into the distilled
//! directory: the bytes the log names, or a finding when the node that makes
//! the artifact never ran.

use yunta_core::durable::{self, Durability};
use yunta_core::events::{ArtifactId, FindingCause, FindingSeverity};
use yunta_core::{DistillArtifact, Location, RelativePath};

use super::ProvenanceArtifact;
use crate::run::{RunCtx, RunError};

/// Copies one declared artifact into `dest_dir` under the name the run's own
/// view gives it, and returns its provenance entry. A declared artifact the
/// run never produced is reported as a finding and recorded as missing; the
/// rest of the declarations distill anyway.
pub(super) async fn copy_declared(
    ctx: &RunCtx<'_>,
    held: &crate::artifacts::RunArtifacts<'_>,
    dest_dir: &std::path::Path,
    declaration: &DistillArtifact,
) -> Result<ProvenanceArtifact, RunError> {
    let wanted = ArtifactId::from(&declaration.id);
    // The distilled copy carries the name the run's own view gives
    // it, so a reader opening the knowledge directory and a reader
    // opening the run find the same file name.
    let name = wanted.view_name();
    match held.held(&wanted, Some(&declaration.node)) {
        Some(artifact) => {
            let bytes = held.bytes(artifact).await?;
            let dest = dest_dir.join(&name);
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|source| RunError::Io {
                        context: format!("create `{}`", parent.display()),
                        source,
                    })?;
            }
            // blocking: one distilled artifact, replaced whole.
            durable::replace(&dest, &bytes, Durability::Atomic).map_err(|source| RunError::Io {
                context: format!("write `{}`", dest.display()),
                source,
            })?;
            Ok(ProvenanceArtifact {
                name,
                content_hash: Some(format!("sha256:{}", artifact.content_hash)),
                missing: false,
            })
        }
        None => missing_declared(ctx, declaration, name).await,
    }
}

/// Reports a declared artifact the run never produced as a finding, and records it as missing.
async fn missing_declared(
    ctx: &RunCtx<'_>,
    declaration: &DistillArtifact,
    name: String,
) -> Result<ProvenanceArtifact, RunError> {
    // Declared durable, never produced (a mode excluded its
    // node, a reroute never reached it): registered, never
    // lost — reported as a finding — and the rest distills
    // anyway.
    ctx.engine_finding(
        None,
        &format!("distill-missing-{}-{}", declaration.node, declaration.id),
        FindingSeverity::Minor,
        Location::run(
            RelativePath::of([crate::artifacts::store::view_path(
                Some(&declaration.node),
                &name,
            )]),
            None,
        ),
        FindingCause::DistillMissing {
            node: declaration.node.clone(),
            artifact: declaration.id.clone(),
        },
    )
    .await?;
    Ok(ProvenanceArtifact {
        name,
        content_hash: None,
        missing: true,
    })
}
