//! See [`super`]. What a new run of the workflow chosen usually takes, and what this machine
//! answers for: the panel beside a new run.
//!
//! What it usually takes is what the workflow's past runs here say, and a figure only from as
//! many runs as a percentile needs
//! ([`yunta_replay::history::MIN_SAMPLES`]): below that the panel says how many runs
//! stand, and no figure. The figures are the estimate's own parts, as every reader says them
//! (`spend`), each on a row, then the past runs' tokens as a sparkline, oldest first. What the
//! machine answers for is what `doctor` found at a glance (`doctor::glance`): a row for each
//! adapter, git, the forge and the checkouts, the facts that hold in fewer words than
//! `yunta doctor` says them, and what does not hold in its words whole. Where the panel gives
//! way, what of the machine stands in the way of the chosen workflow's run stays in sight
//! (`in_the_way`).
//!
//! Each fact is read once: the past runs with the catalog, which reads every workflow's
//! history, and the machine as the new run opens; until a fact is read the panel says nothing
//! of it.

use surco::bars::sparkline;
use surco::components::Checklist;
use surco::doc::{Block, Doc};
use surco::line::{Line, Tone};
use surco::look::Look;
use yunta_replay::history::{PriorEstimation, RunSummary};

use crate::page::doctor::{glance, in_the_way, Finding};
use crate::page::Page;
use crate::spend;

/// What a new run of the workflow chosen usually takes, as its past runs here say, and what
/// this machine answers for, as `doctor` finds it at a glance: the panel beside a new run.
/// Not `spend::estimation`, the catalog's one line of it.
pub struct WhatItTakes<'a> {
    /// The workflow's past runs here, oldest first; none until its history is read.
    pub past: Option<&'a [RunSummary]>,
    /// What `doctor` found of this machine, its sessions unopened; none until it is read.
    pub machine: Option<&'a [Finding]>,
}

/// The cells inside the margins of the design's screen: 120 columns, a cell at each side.
const DESIGNS_CELLS: usize = 118;

/// The cells of the edge between the panel and the region beside it: a blank, the upright and
/// a blank.
const EDGE_CELLS: usize = 3;

impl WhatItTakes<'_> {
    /// The panel's heading.
    pub const USUALLY: &'static str = "what it usually takes";

    /// The cells the panel takes beside a new run: a check's mark two cells in, its subject in a
    /// column of eleven cells and two, as wide as `claude-code`, the longest name an adapter a
    /// glance names has, and what it found in twenty-seven, a short clause a row, which wraps
    /// under itself.
    pub const CELLS: usize = 44;

    /// The cells a new run's request takes beside the panel on the design's screen: what the
    /// panel and the edge between them leave of the cells inside its margins. The box a
    /// question is asked in, which a line asked on a page of its own takes at most, its page's
    /// width where that is narrower.
    pub const QUESTION_CELLS: usize = DESIGNS_CELLS - EDGE_CELLS - Self::CELLS;

    /// What of the machine stands in the way of a run of the catalog's workflow `workflow`, a
    /// check a row ([`in_the_way`]): what a new run keeps in sight where the panel gives way.
    /// None until the machine is read, or where nothing stands in the way.
    pub fn in_the_way(&self, workflow: &str) -> Vec<Block> {
        let rows = self
            .machine
            .map(|findings| in_the_way(findings, workflow))
            .unwrap_or_default();
        if rows.is_empty() {
            return Vec::new();
        }
        let mut checklist = Checklist::default();
        checklist.extend(rows);
        vec![checklist.into()]
    }

    /// The heading, then how many past runs stand, and from as many as an estimate needs on what
    /// they usually took and the tokens each spent: each row as wide as the panel.
    fn usually(&self, look: &Look) -> Vec<Line> {
        let mut lines = vec![Line::new().push(Tone::Muted, Self::USUALLY)];
        let Some(past) = self.past else {
            return lines;
        };
        lines.push(Line::new().plain(spend::past_runs(past.len())));
        if let Some(estimation) = PriorEstimation::of(past) {
            let spent: Vec<f64> = past.iter().map(|run| run.tokens as f64).collect();
            lines.extend([
                Line::new().plain(spend::usual_tokens(&estimation)),
                Line::new().plain(spend::usual_wall_clock(&estimation)),
                Line::new().push(
                    Tone::Running,
                    sparkline(&spent, look.width.cells(), look.glyphs),
                ),
            ]);
        }
        lines
    }
}

impl Page for WhatItTakes<'_> {
    /// What it usually takes under its heading, then what the machine answers for, a check a
    /// row.
    fn document(&self, look: &Look) -> Doc {
        let doc = Doc::new().with(Block::Lines(self.usually(look)));
        let rows = self.machine.map(glance).unwrap_or_default();
        if rows.is_empty() {
            return doc;
        }
        let mut checklist = Checklist::default();
        checklist.extend(rows);
        doc.with(checklist)
    }
}
