//! See [`super`]. What a run spent, read at one instant.

use std::collections::BTreeMap;

use yunta_core::PricingEntry;
use yunta_replay::frame::RunFrame;
use yunta_replay::stats::RunStats;
use yunta_replay::RunState;

use super::OpenedRun;

/// What a run spent, read at one instant: where it stands, its tokens, times
/// and attempts by node and runner, and the prices this project costs tokens
/// at.
///
/// Not render's stats page, which a person reads drawn from it.
pub struct StatsReading {
    /// Where the run stands, which a page's headline reads.
    pub frame: RunFrame,
    pub stats: RunStats,
    /// What this project prices each model's tokens at, when it says.
    pub pricing: Option<BTreeMap<String, PricingEntry>>,
}

impl OpenedRun<'_> {
    /// What the run spent at the project's clock's instant: a run still going
    /// counts the wait or the work open now.
    pub fn stats(&self) -> StatsReading {
        let now = self.project.workspace.clock.now();
        let state = RunState::replay(&self.events);
        StatsReading {
            stats: RunStats::at(&state, &self.manifest.doc.workflow, now),
            frame: self.frame_of(&state, now),
            pricing: self.project.config.pricing.clone(),
        }
    }
}
