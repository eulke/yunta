//! The adapter CLIs this machine answers for: every one this binary builds,
//! probed on the environment's `PATH`.

use yunta_core::port::ProbeReport;
use yunta_core::process::SearchPath;
use yunta_core::{AdapterSettings, ProbedAdapter};

/// Probes every adapter this binary builds, each CLI found on `search_path`
/// as a session of a run started here finds it, in the order the composition
/// root declares them — the id each reports about itself, never one
/// re-spelled here.
pub async fn probe_known_adapters(search_path: &SearchPath) -> Vec<ProbedAdapter> {
    let mut probed = Vec::new();
    for adapter in crate::machine::built_adapters(|_| AdapterSettings::default()) {
        let id = adapter.id().clone();
        probed.push(match adapter.probe(search_path).await {
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
