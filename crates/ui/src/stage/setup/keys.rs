//! See [`super`]. The setup's keys beyond its rows: what Enter, `n` and `q` do once it is
//! written, what its palette offers on the pack's audit, what help lists over it as it
//! stands, and where the audit, a page, is scrolled to.

use yunta_core::step::{Arg, NextStep};
use yunta_render::screen::StageKeys;

use super::super::{Act, FirstRunStage, Stage, Want};
use super::{Setting, SetupStage};
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{FollowedRuns, SetupPackFetch};

impl SetupStage {
    /// What the setup offers, with the key that takes it here: on the pack's audit, installing
    /// it, by Enter, as `yunta pack add` does, which its confirmation asks first; nothing
    /// elsewhere, its rows being changed in place.
    pub(crate) fn offered(&self) -> Vec<Act> {
        let Setting::Audit { rows, .. } = &self.setting else {
            return Vec::new();
        };
        match rows.found.pack.as_deref() {
            Some(SetupPackFetch::Fetched { installs_as, .. }) => {
                vec![Act::by(Stroke::Enter, installs_as.clone())]
            }
            _ => Vec::new(),
        }
    }

    /// The keys help lists over the setup, `page` saying those every page answers to: on its
    /// rows, what needs the person by `a`; on the pack's audit, `a`, the keys that scroll it
    /// and `q` leaving with nothing written; once written, a new run by `n`, the repository set
    /// up; nothing else while it is found or written. It leaves by `q`, as its legend says,
    /// and no run is asked for before it is written.
    pub(crate) fn keys<'l>(&self, page: StageKeys<'l>) -> StageKeys<'l> {
        let new_run = page.new_run;
        let quiet = StageKeys {
            new_run: false,
            attends: false,
            scrolls: false,
            down: None,
            ..page
        };
        match &self.setting {
            Setting::Rows(_) => StageKeys {
                attends: true,
                ..quiet
            },
            Setting::Audit { .. } => StageKeys {
                beyond: yunta_render::screen::setup_audit_keys_beyond(),
                attends: true,
                scrolls: true,
                ..quiet
            },
            Setting::Written { .. } => StageKeys { new_run, ..quiet },
            Setting::Finding { .. } | Setting::Unfound | Setting::Writing { .. } => quiet,
        }
    }

    /// Where the pack's audit stands as a page, which no row of its screen says, its legend
    /// the same whether it scrolls or not; none away from the audit.
    pub(crate) fn page(&self) -> Option<Scroll> {
        match &self.setting {
            Setting::Audit { scroll, .. } => Some(*scroll),
            Setting::Finding { .. }
            | Setting::Unfound
            | Setting::Rows(_)
            | Setting::Writing { .. }
            | Setting::Written { .. } => None,
        }
    }
}

/// What `stroke` asks for on what a setup wrote, over what `followed` keeps: Enter goes on to
/// the first request, `n` opens a new run over it, the repository set up, and `q` or Ctrl-C
/// leaves.
pub(super) fn pressed_on_what_was_written(stroke: Stroke, followed: &FollowedRuns) -> Vec<Want> {
    match stroke {
        Stroke::Enter => vec![Want::Replace(Stage::FirstRun(FirstRunStage::new()))],
        Stroke::Insert('n') if followed.can_ask_for_a_run() => Want::taking(&NextStep::Run {
            workflow: Arg::Blank,
        }),
        Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
        _ => Vec::new(),
    }
}
