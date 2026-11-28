//! The facts a page says about a run, each in its row, and the runs it
//! composed.
//!
//! At a glance a page says how far the run has come, where its nodes and
//! tasks stand, what it cautions and what it could not read. A closing
//! adds the pull requests the run opened, what it took, what it changed,
//! where its time went and where its work is; `--full` says every fact;
//! `--dig` adds where the run itself is kept.
//!
//! The rows the field's glance at a run says too (how far it came, what it spent, where its
//! time went, its slowest nodes, its branch, how long it has stood) are worded here once, each
//! label and each value, so `yunta status` and the glance say a fact the same way; a value of
//! several parts takes a comma between two, as the glance has it.

use std::time::Duration;

use crate::children;
use crate::progress;
use crate::spend::Breakdown;
use surco::components::{Diffstat, Fields, Said, Section};
use surco::doc::{Block, Depth};
use surco::figures::duration;
use surco::glyphs::Glyphs;
use surco::line::{Line, Tone};
use surco::paths;
use surco::text::{counted, counted_as};
use surco::words::{AT_WORK, WAITING_ON_YOU};
use yunta_core::Isolation;
use yunta_replay::frame::{NodeFrame, NodeStanding, RunFrame, RunTimes};
use yunta_replay::history::PriorEstimation;
use yunta_replay::TimeSpent;

use super::{caution_row, Run, RunScope};

/// How many of the run's longest nodes the `slowest` row names. Enough to
/// point at where the time went, few enough that the row stays one row.
const SLOWEST_NAMED: usize = 3;

/// The label of how far a run has come.
pub(crate) const PROGRESS: &str = "progress";

/// The label of what a run spent.
pub(crate) const TOKENS: &str = "tokens";

/// The label of where a run's time went, where a person weighed in it.
pub(crate) const TIME: &str = "time";

/// The label of a run's longest nodes.
pub(crate) const SLOWEST: &str = "slowest";

/// The label of the branch a run works on and the one it started from.
pub(crate) const BRANCH: &str = "branch";

impl Run {
    /// The facts with something to say on this page, in the order a reader
    /// checks them. A fact with nothing to say is not a row.
    pub(super) fn facts(&self, glyphs: Glyphs) -> Fields {
        let full = self.depth.reaches(Depth::Full);
        let harvest = self.holds_harvest();
        let mut fields = Fields::new().push_if(PROGRESS, progress::row(&self.frame));
        // The whole story names every node and task in a table of its own.
        if !full {
            for place in progress::places(&self.frame) {
                fields = fields.push_marked("nodes", place.standing.mark(), place.said);
            }
            let tasks = progress::tasks_row(&self.frame, &self.state.tasks, glyphs);
            fields = fields.push_if("tasks", tasks.unwrap_or_default());
        }
        for left in self.left_out() {
            fields = fields.push_if("left out", left);
        }
        if harvest {
            fields = self.came_to(fields, glyphs);
        }
        fields = fields.push_if("caution", caution_row(&self.frame));
        if harvest {
            fields = self.went_to(fields);
        }
        if full {
            fields = fields
                .push_if("environment", self.environment())
                .push_if("host", self.suspended());
        }
        fields = fields.push_if(
            "unread",
            crate::state::unknown_kinds(&self.frame.unknown_kinds).unwrap_or_default(),
        );
        if self.depth.reaches(Depth::Dig) {
            fields = fields.push_if("id", self.run_id.to_string()).push_if(
                "kept in",
                paths::shown(
                    &self.places.run_dir,
                    &self.places.cwd,
                    self.places.home.as_deref(),
                ),
            );
        }
        fields
    }

    /// Why each node the run left out is not in it — what the project
    /// lacks — said whole: the table's note has the room the line leaves.
    fn left_out(&self) -> Vec<String> {
        self.frame
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                NodeStanding::LeftOut(because) => Some(format!("{}: {because}", node.id)),
                _ => None,
            })
            .collect()
    }

    /// What the run spent, and what this workflow's own past runs spent —
    /// the only honest comparison there is, because it is what already
    /// happened rather than a prediction. Nothing when the run spent
    /// nothing and has no history to compare with.
    fn spent(&self, glyphs: Glyphs) -> String {
        match &self.frame.prior {
            Some(prior) => format!(
                "{} {} {}",
                crate::spend::Breakdown(&self.frame.tokens),
                glyphs.sep(),
                history(prior, glyphs)
            ),
            None => tokens(&self.frame).unwrap_or_default(),
        }
    }

    /// What the run changed from its base to its newest tree, as git last
    /// counted it on the tree the run stands on. Nothing for a run whose
    /// change was not counted there, or that touched no file.
    pub(super) fn changed(&self, glyphs: Glyphs) -> String {
        self.state
            .current_change()
            .and_then(|change| crate::changes::of(change, glyphs))
            .unwrap_or_default()
    }

    /// Whether this page holds what a run gathered: a closing does, and a
    /// status page as deep as the whole story.
    fn holds_harvest(&self) -> bool {
        matches!(self.scope, RunScope::Closing { .. }) || self.depth.reaches(Depth::Full)
    }

    /// What the run came to: the pull requests it opened, what it spent and
    /// what it changed.
    fn came_to(&self, fields: Fields, glyphs: Glyphs) -> Fields {
        self.frame
            .pull_requests()
            .fold(fields, |fields, pull| {
                fields.push_if(
                    "pull request",
                    crate::outcome::pull_request(pull.number, pull.url),
                )
            })
            .push_if(TOKENS, self.spent(glyphs))
            .push_if("changed", self.changed(glyphs))
    }

    /// Where the run's time went and where its work is.
    fn went_to(&self, fields: Fields) -> Fields {
        fields
            .push_if(
                TIME,
                // Read to the instant the page is read at while the run is
                // open: the log says nothing of a wait until it is answered.
                time_row(&self.times.time).unwrap_or_default(),
            )
            .push_if(
                SLOWEST,
                slowest(&self.frame, &self.times).unwrap_or_default(),
            )
            .push_if(BRANCH, self.branch())
            .push_if("artifacts", self.artifacts())
            .push_if("degraded", self.degraded())
    }

    /// The files the run's change moved most, on a page that holds the change.
    pub(super) fn changed_files(&self) -> Option<Diffstat> {
        if !self.holds_harvest() {
            return None;
        }
        self.state
            .current_change()
            .and_then(crate::changes::by_file)
    }

    /// The branch the run worked on and where that work sits now.
    fn branch(&self) -> String {
        let tree = paths::shown(
            &self.places.worktree,
            &self.places.cwd,
            self.places.home.as_deref(),
        );
        match self.places.isolation {
            Isolation::Worktree => format!(
                "{}, in {tree}",
                off(&self.places.branch, &self.places.base_branch),
            ),
            Isolation::None => match tree.as_str() {
                "." => format!("{}, in this checkout", self.places.base_branch),
                _ => format!("{}, in the checkout at {tree}", self.places.base_branch),
            },
        }
    }

    /// How many artifacts the run's nodes wrote, and the one directory
    /// they are under. Nothing for a run that wrote none.
    fn artifacts(&self) -> String {
        let written = self.frame.artifacts_written();
        if written == 0 {
            return String::new();
        }
        format!(
            "{} under {}",
            counted(written, "artifact"),
            paths::shown(
                &yunta_core::run_dir::artifacts(&self.places.run_dir),
                &self.places.cwd,
                self.places.home.as_deref()
            )
        )
    }

    /// How many capabilities the run asked an adapter for and did without.
    fn degraded(&self) -> String {
        match self.frame.degraded.len() {
            0 => String::new(),
            n => counted_as(
                n,
                "capability the adapter lacks",
                "capabilities the adapter lacks",
            ),
        }
    }

    /// How the machine the run resumed on differs from the one it started
    /// on, when it does.
    fn environment(&self) -> String {
        self.state
            .run
            .environment_drift()
            .map(|drift| crate::outcome::environment_drift(&drift))
            .unwrap_or_default()
    }

    /// How long the host slept under the run, which its durations leave
    /// out.
    fn suspended(&self) -> String {
        self.state
            .run
            .suspensions()
            .summary()
            .map(|(times, slept)| {
                format!(
                    "suspended {} for {} in all, which durations leave out",
                    counted(times, "time"),
                    duration(slept)
                )
            })
            .unwrap_or_default()
    }

    /// The runs this one composed, each under the node that bore it — on a
    /// closing and in the whole story.
    ///
    /// A tree and never an average, because averaging heterogeneous
    /// children is the lying percentage under another name. A child is a
    /// run of its own, with its own id to go and read. Nothing for a run
    /// that composed none, which is most of them.
    pub(super) fn children(&self, glyphs: Glyphs) -> Vec<Block> {
        if !self.holds_harvest() || self.frame.children.is_empty() {
            return Vec::new();
        }
        let mut blocks = vec![Block::Heading("children".to_string())];
        for (under, born) in children::children_by_node(&self.frame) {
            blocks.push(
                Section {
                    lead: None,
                    title: Line::new().push(Tone::Plain, under),
                    blocks: vec![Said(
                        born.iter()
                            .map(|child| children::child_row(child, glyphs))
                            .collect(),
                    )
                    .into()],
                }
                .into(),
            );
        }
        blocks
    }
}

/// Where a run's time went, where a person weighed in it: what was at work on it,
/// and what waited on a person, a part under a second being no part. Nothing for
/// a run no person held up: the row of its slowest nodes says where its time went.
pub(crate) fn time_row(time: &TimeSpent) -> Option<String> {
    if time.people.as_secs() == 0 {
        return None;
    }
    let waited = format!("{} {WAITING_ON_YOU}", duration(time.people));
    Some(match time.working.as_secs() {
        0 => waited,
        _ => format!("{} {AT_WORK}, {waited}", duration(time.working)),
    })
}

/// What the run framed as `frame` spent, settled and in flight, split in and out: nothing for
/// a run that spent none. Every page reads this one figure, from the frame the run's log
/// derives.
pub(crate) fn tokens(frame: &RunFrame) -> Option<String> {
    let spend = &frame.tokens;
    (spend.total() > 0).then(|| Breakdown(spend).to_string())
}

/// The longest nodes of the run framed as `frame`, longest first, each with how long it took
/// as `times` measures it. `None` for a run where no node ever started.
pub(crate) fn slowest(frame: &RunFrame, times: &RunTimes) -> Option<String> {
    let mut timed: Vec<(&NodeFrame, Duration)> = frame
        .nodes
        .iter()
        .filter_map(|node| {
            let elapsed = times.node(&node.id).elapsed;
            elapsed.map(|elapsed| (node, elapsed))
        })
        .collect();
    if timed.is_empty() {
        return None;
    }
    timed.sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));
    Some(
        timed
            .iter()
            .take(SLOWEST_NAMED)
            .map(|(node, elapsed)| format!("{} {}", node.id, duration(*elapsed)))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// A run's `branch` and the `base` it started from, as a page names the two.
pub(crate) fn off(branch: &str, base: &str) -> String {
    format!("{branch} off {base}")
}

/// How long a run has stood where it is, `stood` as a page says that time.
pub(crate) fn stood_for(stood: impl std::fmt::Display) -> String {
    format!("for {stood}")
}

/// The whole span of a run that closed, `took`, as its screen's headline and its printed
/// page both say it, however it closed: `in 7m41s`.
pub(crate) fn closed_in(took: Duration) -> String {
    format!("in {}", surco::figures::duration(took))
}

/// What this workflow's past runs cost, in the one phrasing every surface
/// that reports it uses.
fn history(prior: &PriorEstimation, glyphs: Glyphs) -> String {
    crate::spend::estimation(prior, glyphs)
}
