//! The adapter CLIs this machine answers for: every one this binary builds,
//! probed.

use yunta_core::port::ProbeReport;
use yunta_core::{AdapterSettings, ProbedAdapter};

/// Probes every adapter this binary builds, in the order the composition
/// root declares them — the id each reports about itself, never one
/// re-spelled here.
pub async fn probe_known_adapters() -> Vec<ProbedAdapter> {
    let mut probed = Vec::new();
    for adapter in crate::machine::built_adapters(|_| AdapterSettings::default()) {
        let id = adapter.id().clone();
        probed.push(match adapter.probe().await {
            Ok(ProbeReport::Healthy { version }) => ProbedAdapter {
                id,
                healthy: true,
                detail: version.unwrap_or_else(|| "version unknown".to_string()),
            },
            Ok(ProbeReport::Unhealthy { diagnostic }) => ProbedAdapter {
                id,
                healthy: false,
                detail: diagnostic,
            },
            Err(e) => ProbedAdapter {
                id,
                healthy: false,
                detail: e.to_string(),
            },
        });
    }
    probed
}
