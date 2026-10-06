//! What no session of a run may write: what the project denies to every
//! run, and the tests a person approved — the files the run's spec gives
//! its tasks.
//!
//! A person approves a task's tests beside the plan, and the work they
//! judge is what makes them pass. Denied to that task alone, a later
//! node — a fix for a lint error, for a finding — could still change
//! them, and the pull request would carry tests nobody approved. Every
//! fence and every close of the run reads the same answer, from here.

use yunta_core::{Node, NodeKind, ScopeGlob, SpecFile};

use super::{RunCtx, RunError};

/// The run's answer to what may never be written, as the log stands.
pub(crate) struct Denied {
    project: Vec<ScopeGlob>,
    tests: Vec<ScopeGlob>,
    /// The configuration files of every CLI the run can drive, which no
    /// session writes whichever CLI it runs on.
    agents: Vec<ScopeGlob>,
}

impl Denied {
    /// What the run denies as its log stands now: the project's deny,
    /// and every file of the spec the run accepted last.
    pub(crate) async fn of(ctx: &RunCtx<'_>) -> Result<Self, RunError> {
        let events = ctx.load_events().await?;
        let tests = crate::artifacts::latest::<SpecFile>(ctx.run_dir, &events)
            .await?
            .map(|held| held.document.test_globs())
            .unwrap_or_default();
        let mut agents: Vec<ScopeGlob> = ctx
            .adapters
            .values()
            .flat_map(|adapter| adapter.own_config())
            .collect();
        agents.sort();
        agents.dedup();
        Ok(Denied {
            project: ctx.manifest.config.denied_paths().to_vec(),
            tests,
            agents,
        })
    }

    /// Everything no session may write.
    pub(crate) fn every(&self) -> Vec<ScopeGlob> {
        self.project
            .iter()
            .chain(&self.agents)
            .chain(&self.tests)
            .cloned()
            .collect()
    }

    /// What `node`'s close refuses in the run's tree. A loop lands its
    /// tasks' work, each task's tests with it, and each task answered
    /// for every test when it landed — so the loop answers only for what
    /// the project denies.
    pub(crate) fn closing(&self, node: &Node) -> Vec<ScopeGlob> {
        match node.kind {
            NodeKind::Loop { .. } => self.project.iter().chain(&self.agents).cloned().collect(),
            _ => self.every(),
        }
    }
}
