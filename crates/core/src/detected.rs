//! What a repository says about itself before anyone configures Yunta for
//! it, and what the machine it is on answers for: the ecosystem it is in,
//! the commands that ecosystem answers for lint, tests and the rest, the
//! suite a run can measure, the forge its `origin` is on, and the adapter
//! CLIs that answered a probe. A caller reads them and a page says them;
//! nothing here reads.

use std::collections::BTreeMap;

use crate::{AdapterId, CommandName, GitHubRepo};

/// What one repository was found to have.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Detected {
    pub ecosystem: Option<Ecosystem>,
    /// What the repository runs for each capability, by the name a
    /// workflow asks for it with.
    pub commands: BTreeMap<CommandName, String>,
    /// The suite a run measures before its first node.
    pub suite: Option<String>,
    /// The GitHub repository `origin` names.
    pub forge: Option<GitHubRepo>,
    /// The adapter CLIs this machine answers for, when something asked —
    /// probing spawns each one, so only a caller with a runner to propose
    /// does.
    pub adapters: MachineAdapters,
}

/// An ecosystem a repository is in, and what to know about building in
/// one checkout per task there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ecosystem {
    pub name: &'static str,
    /// A tip printed to the terminal, never written to config — no key
    /// for it exists in the schema.
    pub cache_tip: &'static str,
}

/// What probing one adapter a build knows found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedAdapter {
    pub id: AdapterId,
    pub healthy: bool,
    /// The version a healthy one reported, or why one is not.
    pub detail: String,
}

/// The adapter CLIs this machine answered for, among every one its build
/// knows: what a runner to declare is proposed on. Not the adapters a
/// config's `runners:` name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MachineAdapters {
    /// The ones that answered healthy.
    pub healthy: Vec<AdapterId>,
    /// Every adapter the build knows, healthy or not.
    pub known: Vec<AdapterId>,
}

impl MachineAdapters {
    /// What probing every adapter a build knows found, as the ones that
    /// answered among them.
    pub fn probed(probed: &[ProbedAdapter]) -> Self {
        MachineAdapters {
            healthy: probed
                .iter()
                .filter(|adapter| adapter.healthy)
                .map(|adapter| adapter.id.clone())
                .collect(),
            known: probed.iter().map(|adapter| adapter.id.clone()).collect(),
        }
    }
}
