//! Where a run stands, derived from exactly its own event log and the
//! manifest that run froze.
//!
//! One derivation, three surfaces: `yunta status` prints
//! [`Progress::summary`] for one run, `yunta run --follow` reprints it
//! whenever it changes, and `yunta list --runs` groups by
//! [`Progress::phase`] and prints the same summary under each row. A
//! group that said one thing while the line under it said another would
//! be a listing nobody could act on, so both read the same value.
//!
//! Counters with context, never percentages: a percentage lies the
//! moment a re-route grows the denominator.

use yunta_core::events::{run_mode, EventPayload, StoredEvent, TaskStatus, TerminalState};
use yunta_core::{Manifest, ModeName, NodeId};
use yunta_engine::NodeState;

/// Where a run as a whole stands.
///
/// The variants are the answers a reader acts on differently: nothing
/// has started, it moves on its own, it stopped until a person acts, it
/// closed, or its log stopped making sense. Each carries the evidence
/// the log has for it, so no surface has to go back for the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The log carries a `run_created` and nothing that started work.
    Created,
    /// A node is in flight.
    Running,
    /// Stopped on a person, carrying the `run_paused` reason that names
    /// what it is stopped on — a gate, an exhausted re-route, a budget
    /// cap, an unanswered questions artifact.
    Waiting { reason: String },
    /// `run_finished`, as the terminal state the run closed with.
    Closed { terminal: TerminalState },
    /// Replay stopped making sense of the log at the event the
    /// diagnostic names; the counters are what it derived before then.
    Broken { diagnostic: String },
}

impl Phase {
    /// The phase on one line, for the end of a summary.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Created => "created".to_string(),
            Self::Running => "running".to_string(),
            // A paused run is waiting on a person, not stuck — "waiting
            // on gate approve-plan" is what a `run_paused` reason
            // already reads like, so this reuses it verbatim rather
            // than inventing a second vocabulary for the same fact.
            Self::Waiting { reason } => {
                format!("waiting — {}", yunta_core::text::one_line(reason))
            }
            Self::Closed { terminal } => closed_word(*terminal).to_string(),
            Self::Broken { diagnostic } => format!("broken — {diagnostic}"),
        }
    }

    /// Which part of a listing this run belongs in.
    pub(crate) fn standing(&self) -> Standing {
        match self {
            // A log that stopped making sense needs a person as much as
            // a decision does: nothing moves it on its own again.
            Self::Waiting { .. } | Self::Broken { .. } => Standing::NeedsYou,
            Self::Created | Self::Running => Standing::InFlight,
            Self::Closed { .. } => Standing::Closed,
        }
    }
}

/// How a closed run closed. `Done` reads as `finished`, the word every
/// other surface already reports a completed run with; the other three
/// are named as themselves, because a run a person cancelled and a run
/// that failed are not a run that finished.
fn closed_word(terminal: TerminalState) -> &'static str {
    match terminal {
        TerminalState::Done => "finished",
        TerminalState::Failed => "failed",
        TerminalState::Cancelled => "cancelled",
        TerminalState::Promoted => "promoted",
    }
}

/// The part of a listing a run belongs in — the inbox's own grouping,
/// derived from the phase so a heading can never disagree with the
/// summary printed under it.
///
/// The order of the variants is the order the groups print in: what
/// stopped on a person comes before what is still moving, which comes
/// before what is already closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Standing {
    /// Stopped until a person acts: a decision to answer, or a log that
    /// stopped making sense.
    NeedsYou,
    /// Moving on its own, or created and not yet started.
    InFlight,
    /// Closed, however it closed.
    Closed,
}

impl Standing {
    /// Every group, in printing order.
    pub(crate) const ALL: [Standing; 3] =
        [Standing::NeedsYou, Standing::InFlight, Standing::Closed];

    /// The heading a group of runs prints under. It says what the reader
    /// can do about the rows below it, which is what a listing is read
    /// for.
    pub(crate) fn heading(self) -> &'static str {
        match self {
            Self::NeedsYou => "needs you",
            Self::InFlight => "in flight",
            Self::Closed => "closed",
        }
    }
}

/// A run's progress: the phase it is in and the counters that qualify
/// it, both derived from exactly the events given and the run's own
/// frozen manifest.
pub(crate) struct Progress {
    pub(crate) phase: Phase,
    /// The mode frozen in `run_created` — the narrowing this run was
    /// created with, which is what the denominator below is measured
    /// against.
    pub(crate) mode: ModeName,
    nodes_terminated: usize,
    /// Declared nodes this run's mode includes.
    denominator: usize,
    skipped: usize,
    waiting: usize,
    tasks: Option<(usize, usize)>,
    reroutes: usize,
    unknown_kinds: Option<String>,
}

impl Progress {
    /// Derives a run's progress from its log and its frozen manifest.
    pub(crate) fn of(events: &[StoredEvent], manifest: &Manifest) -> Self {
        let state = yunta_engine::derive(events);
        let declared: Vec<NodeId> = manifest
            .workflow
            .iter_nodes()
            .map(|node| node.id.clone())
            .collect();
        // The run's mode narrows the denominator — a change that must be
        // visible and attributable, never silent. The mode comes from
        // `run_created`, frozen there at creation; the excluded nodes
        // are counted as skipped, not dropped from the count.
        let mode = run_mode(events);
        let skipped = match yunta_engine::mode_included_nodes(&manifest.workflow, &mode) {
            Some(included) => declared.iter().filter(|id| !included.contains(*id)).count(),
            None => 0,
        };
        let tasks_done = state
            .tasks
            .values()
            .filter(|status| matches!(status, TaskStatus::Done))
            .count();
        Progress {
            phase: phase(events, &state),
            mode,
            nodes_terminated: count_nodes(&state, |node| {
                matches!(node, NodeState::Finished { .. } | NodeState::Failed { .. })
            }),
            denominator: declared.len() - skipped,
            skipped,
            waiting: count_nodes(&state, |node| matches!(node, NodeState::Waiting { .. })),
            tasks: (!state.tasks.is_empty()).then_some((tasks_done, state.tasks.len())),
            reroutes: events
                .iter()
                .filter(|event| matches!(event.payload(), Some(EventPayload::NodeRerouted(_))))
                .count(),
            unknown_kinds: crate::commands::unknown_kinds_note(&yunta_engine::unknown_kind_counts(
                &state,
            )),
        }
    }

    /// The counters and the phase on one line: two levels — flow (nodes
    /// terminated over the DAG the manifest froze) and task (ledger
    /// tasks done over registered) — each a counter with context.
    pub(crate) fn summary(&self) -> String {
        let mut summary = format!("{}/{} nodes", self.nodes_terminated, self.denominator);
        if self.skipped > 0 {
            summary.push_str(&format!(
                " · {} skipped (mode: {})",
                self.skipped, self.mode
            ));
        }
        if self.waiting > 0 {
            summary.push_str(&format!(" · {} waiting", self.waiting));
        }
        if let Some((done, total)) = self.tasks {
            summary = format!("{done}/{total} tasks · {summary}");
        }
        summary.push_str(&format!(
            " · {} reroutes · {}",
            self.reroutes,
            self.phase.label()
        ));
        if let Some(note) = &self.unknown_kinds {
            summary.push_str(&format!(" · {note}"));
        }
        summary
    }
}

/// Counters with context, never percentages — derived from exactly
/// `events` and the run's own frozen `manifest`. Shared by
/// `yunta status` (one run, in detail), `yunta run --follow` (reprinted
/// whenever it changes) and `yunta list --runs` (every local run, under
/// its own row) so no two of them can disagree about what a run's
/// progress means.
pub(crate) fn progress_summary(events: &[StoredEvent], manifest: &Manifest) -> String {
    Progress::of(events, manifest).summary()
}

fn count_nodes(state: &yunta_engine::RunState, is: impl Fn(&NodeState) -> bool) -> usize {
    state.nodes.values().filter(|node| is(node)).count()
}

/// The run's phase: a log that stopped making sense says so first, then
/// the last run-level event that moved the run — its close, its pause,
/// or the start of work.
fn phase(events: &[StoredEvent], state: &yunta_engine::RunState) -> Phase {
    if let Some(diagnostic) = &state.broken {
        return Phase::Broken {
            diagnostic: diagnostic.clone(),
        };
    }
    events
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            Some(EventPayload::RunFinished(p)) => Some(Phase::Closed {
                terminal: p.terminal_state,
            }),
            Some(EventPayload::RunPaused(p)) => Some(Phase::Waiting {
                reason: p.reason.clone(),
            }),
            Some(EventPayload::RunResumed(_) | EventPayload::NodeStarted(_)) => {
                Some(Phase::Running)
            }
            _ => None,
        })
        .unwrap_or(Phase::Created)
}
