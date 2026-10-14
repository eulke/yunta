//! The knobs a run was frozen with, which every decision reads, and the mode
//! a run promotes to next.

use std::collections::HashSet;
use std::sync::Arc;

use yunta_core::protected::Protected;
use yunta_core::{DefaultOnFailure, ModeName, NodeId, OnInterrupt, Workflow};

/// The mode immediately after `mode_name` in `modes:`'s own declaration
/// order — the *only* direction promotion ever moves (going back to an
/// earlier mode in the declaration doesn't exist), and the smallest
/// possible escalation past the current one, rather than jumping
/// straight to whichever mode a human might name off-hand.
/// `None` when the workflow declares no modes, the current mode name
/// isn't one of them (the `"default"` sentinel, or a stale/renamed
/// mode), or it's already the last one declared — nothing to promote
/// to, so promotion is never offered.
pub fn next_mode_after(workflow: &Workflow, mode_name: &ModeName) -> Option<ModeName> {
    let modes = workflow.modes.as_ref()?;
    let index = modes.get_index_of(mode_name)?;
    modes.get_index(index + 1).map(|(name, _)| name.clone())
}

/// What the run was frozen with: the knobs every decision reads.
/// Built once, when the run wakes, and handed to [`super::decide`] unchanged
/// for the rest of the invocation — a decision that re-read them could
/// answer two different things about the same log.
#[derive(Debug, Clone)]
pub struct Policy {
    /// How many independently-ready nodes may run at once.
    pub max_parallel_nodes: u32,
    /// What a node with no `on_interrupt` of its own inherits.
    pub on_interrupt: OnInterrupt,
    /// What a failed node with no `on_failure` of its own does to the run.
    pub on_failure: DefaultOnFailure,
    /// The nodes this run schedules — its mode's, less what it left out
    /// for lack of config; `None` when nothing narrows the graph.
    pub mode_nodes: Option<HashSet<NodeId>>,
    /// The suite this run's lineage measures, from `baseline.suite`;
    /// `None` when the config names none and nothing is measured.
    pub baseline_suite: Option<String>,
    /// The commands `baseline.held` names, measured beside the suite.
    pub baseline_held: Vec<(yunta_core::CommandName, String)>,
    /// Whether the suite is measured in a checkout of its own while the
    /// run goes on: a run born in a checkout holding exactly the commit
    /// it opened on. Otherwise it is measured in the run's own tree,
    /// before anything of the run's changes it.
    pub measures_aside: bool,
    /// Whether a later mode follows this run's, so that an exhausted
    /// re-route offers to promote to it — and a successor is born holding
    /// this run's measurement.
    pub may_promote: bool,
    /// Whether a person may widen a node's scope from the menu of a
    /// failure it had on that scope: `false` only under a permission
    /// ceiling of `scope_expansion.max_mode: deny`.
    pub grants_scope: bool,
    /// What no session of the run may write: a grant that reaches it is
    /// never offered.
    pub protected: Arc<Protected>,
    /// Whether the run may move into the checkout its last unit built in:
    /// it works in a checkout of the project's pool of its own.
    pub follows_builds: bool,
}

impl Policy {
    /// The knobs `manifest` froze, for the mode this invocation runs and
    /// the nodes its birth left out, with the set of paths no session may
    /// write. Built once when the run wakes: the node set is a pure
    /// function of the workflow, the mode and what the log recorded, and
    /// re-deriving it per decision would be the same answer at scheduler
    /// cost.
    pub fn of(
        manifest: &yunta_core::Manifest,
        mode_name: &ModeName,
        run: &yunta_core::events::RunLedger,
        protected: Arc<Protected>,
    ) -> Self {
        Policy {
            max_parallel_nodes: manifest.max_parallel_nodes,
            on_interrupt: manifest.config.resolved_on_interrupt(),
            on_failure: manifest.config.resolved_on_failure(),
            mode_nodes: crate::modes::included_nodes(&manifest.workflow, mode_name, run.left_out()),
            baseline_suite: crate::run::baseline::suite(manifest).map(str::to_string),
            baseline_held: crate::run::baseline::held(manifest),
            measures_aside: run.opens_on_base(),
            may_promote: next_mode_after(&manifest.workflow, mode_name).is_some(),
            grants_scope: person_may_grant_scope(&manifest.config),
            protected,
            follows_builds: manifest.isolation == yunta_core::Isolation::Worktree
                && run.checkout().is_some(),
        }
    }
}

/// Whether a person may widen a node's scope on this run: every layer
/// allows it but a permission ceiling of `scope_expansion.max_mode:
/// deny`. The one reading of that rule — the menu, the node's close and
/// its session all ask here.
pub(crate) fn person_may_grant_scope(config: &yunta_core::ConfigLayer) -> bool {
    config
        .permissions
        .as_ref()
        .and_then(|permissions| permissions.scope_expansion)
        .is_none_or(|ceiling| ceiling.max_mode.lets_a_person_grant())
}
