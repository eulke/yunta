//! See [`super`]. The adapters this binary builds: the composition root's one
//! declaration of what a real invocation can run on.

use std::collections::HashMap;
use std::sync::Arc;

use yunta_adapters::{ClaudeCodeAdapter, CodexAdapter, CLAUDE_CODE_ID, CODEX_ID};
use yunta_core::check::AdapterFacts;
use yunta_core::port::Adapter;
use yunta_core::{AdapterId, AdapterSettings, ConfigLayer};

/// The adapters a run executes its sessions on, by the name `runners:`
/// reaches each one under. Named beside the function that builds it, so
/// every caller that passes a registry around spells the same type.
pub type Adapters = HashMap<AdapterId, Arc<dyn Adapter>>;

/// Every adapter this binary builds, with `settings` applied to each —
/// the composition root's one declaration of what a real invocation can
/// run on. Adding an adapter is adding a line here: what `runners:` may
/// name, what `doctor` probes, what `init` offers and what a refusal
/// lists all read from this.
///
/// The mock is not among them. Mock fixtures stay routed through `yunta
/// test`, so a real run never gets a simulated agent.
pub fn built_adapters(settings: impl Fn(&AdapterId) -> AdapterSettings) -> Vec<Arc<dyn Adapter>> {
    vec![
        Arc::new(ClaudeCodeAdapter::new(&settings(&CLAUDE_CODE_ID))),
        Arc::new(CodexAdapter::new(&settings(&CODEX_ID))),
    ]
}

/// Every variable a built adapter reaches its service by, once each: what
/// the shell captures so a probe reaches the endpoint the adapter's CLI
/// will, without anything below the shell reading the process.
pub fn service_vars() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = built_adapters(|_| AdapterSettings::default())
        .iter()
        .flat_map(|adapter| adapter.service_vars().iter().copied())
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// What every adapter this binary builds declares it can do, and what its
/// sandbox is set to for an `edit` session, each under the settings the
/// config gives it: what `check` judges a workflow's `permissions:` and
/// `agent:` against. `None` for an adapter this binary does not build: a
/// capability it cannot see is not one it can call absent.
pub fn declared_facts(config: &ConfigLayer) -> impl Fn(&AdapterId) -> Option<AdapterFacts> {
    let configured = config.adapters.clone().unwrap_or_default();
    let built_facts: Vec<(AdapterId, AdapterFacts)> =
        built_adapters(|id| configured.get(id).cloned().unwrap_or_default())
            .iter()
            .map(|built| {
                let facts = AdapterFacts {
                    capabilities: built.capabilities(),
                    edit_sandbox: built.edit_sandbox(),
                };
                (built.id().clone(), facts)
            })
            .collect();
    move |id| {
        built_facts
            .iter()
            .find(|(built, _)| built == id)
            .map(|(_, facts)| facts.clone())
    }
}

/// The adapter this binary built under `id`, constructed without a
/// probe: what a rail hook needs to reach one adapter's codec.
pub fn built_adapter(id: &AdapterId) -> Option<std::sync::Arc<dyn Adapter>> {
    built_adapters(|_| AdapterSettings::default())
        .into_iter()
        .find(|built| built.id() == id)
}

/// What this binary can run on, as a person reads it: every built
/// adapter's own id, in order, comma-separated — the phrase a refusal
/// ends with, derived rather than written.
pub fn built_adapter_names() -> String {
    surco::text::listed(built_adapter_ids().iter().map(AdapterId::as_str))
}

/// Every adapter this binary builds, by its own id, in the order the
/// composition root declares them.
pub fn built_adapter_ids() -> Vec<AdapterId> {
    built_adapters(|_| AdapterSettings::default())
        .iter()
        .map(|adapter| adapter.id().clone())
        .collect()
}

/// The id a surface names when it shows what a `runners:` entry looks
/// like. The first this binary builds, so the example is always an
/// adapter that exists.
pub fn first_built_adapter() -> AdapterId {
    built_adapters(|_| AdapterSettings::default())
        .first()
        .map(|adapter| adapter.id().clone())
        .unwrap_or_else(|| CLAUDE_CODE_ID.clone())
}

/// The adapters a real invocation offers: those of [`built_adapters`]
/// that `runners:` names as a candidate somewhere in the merged config,
/// each with that adapter's own settings (a `binary` override, if
/// declared).
pub fn real_adapters(config: &ConfigLayer) -> Adapters {
    let named: Vec<&AdapterId> = config
        .runners
        .iter()
        .flatten()
        .flat_map(|(_, candidates)| candidates.iter())
        .map(|candidate| &candidate.adapter)
        .collect();

    built_adapters(|id| {
        config
            .adapters
            .as_ref()
            .and_then(|adapters| adapters.get(id))
            .cloned()
            .unwrap_or_default()
    })
    .into_iter()
    .filter(|adapter| named.contains(&adapter.id()))
    .map(|adapter| (adapter.id().clone(), adapter))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example a surface offers is an adapter that exists, so
    /// copying the line it prints produces a config this binary can run.
    #[test]
    fn the_example_a_surface_offers_is_an_adapter_this_binary_builds() {
        let example = first_built_adapter();
        assert!(
            built_adapters(|_| AdapterSettings::default())
                .iter()
                .any(|adapter| *adapter.id() == example),
            "`{example}` is offered as an example and is not built"
        );
    }
}
