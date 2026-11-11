//! See [`super`]. What `doctor` checks of this machine and this project, a
//! check each, in the order it runs them.
//!
//! Every adapter the project's `runners:` names is probed: the binary there
//! and answering `--version`, every setting read. The same `probe()` a real
//! run calls before spending anything; here every result is a check of its
//! own instead of a refusal.
//!
//! Who git commits a run's work as, or that it cannot name anyone. What the
//! forge the config declares answers (`forge`). What the project's
//! checkouts keep of the disk: within their budget, and over the part of it
//! always kept free, under which a run stops. On macOS, what a new binary's
//! first run costs here (`first_run`). Every installed pack's own
//! `requires:` against the project's merged config, and its workflows as a
//! run would check them (`packs`). And, when asked, one real session per
//! binding (`session`).

mod first_run;
mod forge;
mod packs;
mod session;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::sync::Arc;

use yunta_core::check::CheckError;
use yunta_core::port::ProbeReport;
use yunta_core::{AdapterId, ConfigKey, Detected, MachineAdapters};
use yunta_engine::{CheckoutPool, PoolReport};
use yunta_render::declare;
use yunta_render::page::doctor::{AdapterHealth, CheckoutsKeep, Finding};

use crate::{Committer, Error, Project, Remarks, Stop};

impl Project {
    /// What every check `doctor` makes of this machine and this project
    /// found, in the order it makes them; under `session`, one real session
    /// per binding whose adapter answered. What the checks say while they
    /// run is said through `remarks`.
    pub async fn doctor(
        &self,
        session: bool,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Vec<Finding>, Error> {
        let mut findings = Vec::new();
        let (healthy, answered) = self.probe_adapters(&mut findings).await;
        findings.push(match self.workspace.committer(stop).await? {
            Committer::Named(who) => Finding::Committer { who },
            Committer::Unnamed { git_said } => Finding::NoCommitter { git_said },
        });
        findings.extend(forge::findings(self, remarks, stop).await?);
        findings.push(self.checkouts(stop).await);
        findings.extend(first_run::first_run(&self.workspace.env).await);
        // A runner a pack lacks is declared on an adapter that answers here.
        packs::check_requires(self, &answered, &mut findings);
        packs::check_workflows(self, &answered, &mut findings, stop).await?;
        if session {
            self.probe_sessions(&healthy, &mut findings, remarks, stop)
                .await;
        }
        Ok(findings)
    }

    /// What the project's checkouts keep of the disk.
    async fn checkouts(&self, stop: &Stop) -> Finding {
        let limits = self
            .config
            .storage
            .as_ref()
            .and_then(|storage| storage.pool.as_ref());
        let clock = Arc::clone(&self.workspace.clock);
        let pool = CheckoutPool::upkeep(&self.worktrees_root, &self.workspace.dir, limits, clock);
        match pool.report(self.workspace.supervision(stop)).await {
            Ok(kept) => Finding::Checkouts {
                kept: kept.to_string(),
                keep: keep_of(&kept),
            },
            Err(e) => Finding::CheckoutsUnread {
                said: e.to_string(),
            },
        }
    }

    /// Probes every adapter this project's `runners:` names, a finding each.
    /// Hands back the ones that answered healthy, the only ones a session is
    /// worth opening on, and the adapters a runner to declare would run on:
    /// those, or every adapter CLI this machine answers for when none of
    /// them did.
    async fn probe_adapters(
        &self,
        findings: &mut Vec<Finding>,
    ) -> (BTreeSet<AdapterId>, MachineAdapters) {
        let adapters = super::real_adapters(&self.config);
        if adapters.is_empty() {
            let answered = self.no_runner_declared(findings).await;
            return (BTreeSet::new(), answered);
        }
        let mut healthy = BTreeSet::new();
        let mut names: Vec<&AdapterId> = adapters.keys().collect();
        names.sort();
        for name in names {
            let Some(adapter) = adapters.get(name) else {
                continue;
            };
            let health = health(adapter.probe(&self.workspace.env.path).await);
            if matches!(health, AdapterHealth::Healthy { .. }) {
                healthy.insert(name.clone());
            }
            findings.push(Finding::Adapter {
                adapter: name.clone(),
                health,
            });
        }
        let answered = match healthy.is_empty() {
            true => MachineAdapters::probed(
                &super::probe_known_adapters(&self.workspace.env.path).await,
            ),
            false => MachineAdapters {
                healthy: healthy.iter().cloned().collect(),
                known: super::built_adapter_ids(),
            },
        };
        (healthy, answered)
    }

    /// What `doctor` finds of a project whose `runners:` names no adapter
    /// this build supports: that nothing has an adapter to run on, and the
    /// runner to declare with the adapters this machine answers for, which
    /// it hands back. A problem when a workflow in the catalog needs one (it
    /// would stop the first time it reached an agent node), and a caution
    /// otherwise.
    async fn no_runner_declared(&self, findings: &mut Vec<Finding>) -> MachineAdapters {
        let needing = self.needing_a_runner();
        let errors: Vec<CheckError> = needing
            .iter()
            .flat_map(|(_, errors)| errors.iter().cloned())
            .collect();
        let answered =
            MachineAdapters::probed(&super::probe_known_adapters(&self.workspace.env.path).await);
        let detected = Detected {
            adapters: answered.clone(),
            ..Detected::default()
        };
        // The errors are all about runners, so what answers them is the one
        // runner to declare.
        let declare = super::suggestions(&errors, &detected)
            .pop()
            .unwrap_or_else(|| declare::runners(&[], true, &answered));
        findings.push(Finding::NoRunner {
            needing: needing.into_iter().map(|(name, _)| name).collect(),
            declare,
        });
        answered
    }

    /// Every catalog workflow that names a runner the config lacks, with
    /// what `check` says about each.
    fn needing_a_runner(&self) -> Vec<(String, Vec<CheckError>)> {
        let facts = super::declared_facts(&self.config);
        self.workspace
            .catalog_workflows()
            .into_iter()
            .map(|(name, workflow)| {
                let errors = yunta_engine::check(&workflow, &self.config, &facts)
                    .into_iter()
                    .filter(names_a_runner)
                    .collect();
                (name, errors)
            })
            .filter(|(_, errors): &(String, Vec<_>)| !errors.is_empty())
            .collect()
    }

    /// Opens one session per binding whose adapter probed healthy, a finding
    /// each.
    ///
    /// Only the bindings whose adapter is already healthy: the rest have
    /// nothing a session could add, and the checks above already name them.
    async fn probe_sessions(
        &self,
        healthy: &BTreeSet<AdapterId>,
        findings: &mut Vec<Finding>,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) {
        let bindings: Vec<session::Binding> = session::bindings(&self.config)
            .into_iter()
            .filter(|binding| healthy.contains(&binding.candidate.adapter))
            .collect();
        if bindings.is_empty() {
            findings.push(Finding::NoBinding);
            return;
        }
        for binding in &bindings {
            findings.push(session::session_probe(self, binding, remarks, stop).await);
        }
    }
}

/// How what the checkouts keep stands against the disk: under the part kept
/// free, a run stops; over their budget, the next checkout taken empties the
/// builds past it.
fn keep_of(kept: &PoolReport) -> CheckoutsKeep {
    match (kept.available < kept.keep_free, kept.builds > kept.budget) {
        (true, _) => CheckoutsKeep::DiskUnderFree,
        (false, true) => CheckoutsKeep::BuildsOverBudget,
        (false, false) => CheckoutsKeep::Within,
    }
}

/// What probing an adapter answered.
fn health(answer: Result<ProbeReport, impl std::fmt::Display>) -> AdapterHealth {
    match answer {
        Ok(ProbeReport::Healthy { version }) => AdapterHealth::Healthy { version },
        Ok(ProbeReport::Unhealthy { diagnostic }) => AdapterHealth::Unhealthy { said: diagnostic },
        Err(e) => AdapterHealth::Unhealthy {
            said: e.to_string(),
        },
    }
}

/// Whether `error` is about a runner the config lacks.
fn names_a_runner(error: &CheckError) -> bool {
    matches!(
        error,
        CheckError::UnknownRunner { .. }
            | CheckError::RunnerHasNoCandidates { .. }
            | CheckError::Unset {
                key: ConfigKey::Runner,
                ..
            }
    )
}
