//! `requires:` validated against the installer's own merged config —
//! the mirror image of `declares`:
//! `declares` is a ceiling the pack promises never to exceed, `requires`
//! is a floor the *installer's* config must clear before the pack can
//! actually run anywhere. Pure comparison only: whether a runner name
//! resolves to at least one candidate, whether an `mcp_servers:` name
//! is defined — no adapter probing (`check` doesn't do that for a
//! workflow's own `runner:` either) and no PATH
//! lookup for `requires.programs` (that needs real filesystem access,
//! `yunta doctor`'s job, not this pure function's).

use yunta_core::{ConfigLayer, McpServerName, PackManifest, PackRef, RunnerName};

/// One pack's requirements the local config can't currently satisfy —
/// empty in every field means the pack is fully resolvable as installed.
#[derive(Debug, Clone, PartialEq)]
pub struct PackRequiresGap {
    pub pack: PackRef,
    /// Runner names `requires.runners` names that either aren't defined
    /// under `runners:` at all, or are defined with zero candidates —
    /// same two failure shapes `check`'s own `UnknownRunner`/
    /// `RunnerHasNoCandidates` distinguish for a workflow's own
    /// `runner:` field.
    pub missing_runners: Vec<RunnerName>,
    /// `requires.mcp_servers` names absent from `mcp_servers:`.
    pub missing_mcp_servers: Vec<McpServerName>,
    /// `requires.programs` — passed through untouched; presence on
    /// `PATH` is the caller's own concern (`yunta doctor`).
    pub required_programs: Vec<String>,
}

impl PackRequiresGap {
    pub fn is_satisfied(&self) -> bool {
        self.missing_runners.is_empty() && self.missing_mcp_servers.is_empty()
    }

    /// Everything this gap leaves a run of the pack's workflows without,
    /// with `on_path` answering whether a required program is installed
    /// where the run would look.
    pub fn unmet(&self, on_path: &dyn Fn(&str) -> bool) -> Vec<crate::CheckError> {
        let runners = self.missing_runners.iter().map(|runner| {
            format!(
                "runner `{runner}`, which `runners:` does not define with a candidate — define it"
            )
        });
        let servers = self.missing_mcp_servers.iter().map(|server| {
            format!("MCP server `{server}`, which `mcp_servers:` does not define — define it")
        });
        let programs = self
            .required_programs
            .iter()
            .filter(|program| !on_path(program))
            .map(|program| {
                format!("program `{program}`, which is not on this machine's `PATH` — install it")
            });
        runners
            .chain(servers)
            .chain(programs)
            .map(|requirement| crate::CheckError::PackRequirementUnmet {
                pack: self.pack.to_string(),
                requirement,
            })
            .collect()
    }
}

pub fn check_pack_requires(manifest: &PackManifest, config: &ConfigLayer) -> PackRequiresGap {
    let missing_runners = manifest
        .requires
        .runners
        .iter()
        .filter(|required| {
            config
                .runners
                .as_ref()
                .and_then(|runners| runners.get(&required.name))
                .is_none_or(|candidates| candidates.is_empty())
        })
        .map(|required| required.name.clone())
        .collect();

    let missing_mcp_servers = manifest
        .requires
        .mcp_servers
        .iter()
        .filter(|name| {
            config
                .mcp_servers
                .as_ref()
                .is_none_or(|servers| !servers.contains_key(*name))
        })
        .cloned()
        .collect();

    PackRequiresGap {
        pack: manifest.reference(),
        missing_runners,
        missing_mcp_servers,
        required_programs: manifest.requires.programs.clone(),
    }
}
