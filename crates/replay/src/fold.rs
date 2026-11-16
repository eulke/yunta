//! See [`super`]. How each event of a run's log is taken into the
//! [`RunState`]: the fold and its head, the replay that folds a whole log,
//! and the step that hands an event the fold applies to every ledger its
//! domain reaches.

use yunta_core::events::tasks::ledger::UnknownTask;
use yunta_core::events::{EventMeta, EventPayload, RunEvent, StoredEvent};
use yunta_core::Seq;

use crate::clock::Open;
use crate::{ReplayError, RunState};

impl RunState {
    /// The state `events` leave a run in, each taken in turn by
    /// [`fold`](Self::fold). Pure: same input, same output, always — a
    /// property test relies on exactly this.
    pub fn replay(events: &[StoredEvent]) -> RunState {
        let mut state = RunState::default();
        for event in events {
            state.fold(event);
        }
        state
    }

    /// Takes one event of the run's log, as a reader that follows the run
    /// takes each event it reads.
    ///
    /// An event at or below the head was taken already and moves nothing.
    /// Past it, the head moves to the event's seq. Once the log broke,
    /// nothing else moves. Otherwise the event is applied, and one the log
    /// cannot take marks the state broken with why.
    pub fn fold(&mut self, event: &StoredEvent) {
        if self.has_taken(event.seq) {
            return;
        }
        self.head = Some(event.seq);
        if self.broken.is_some() {
            return;
        }
        if let Err(error) = self.apply(event) {
            self.broken = Some(error.to_string());
        }
    }

    /// The seq of the last event the state took, past a break too: where a
    /// reader that follows the run reads on from. `None` before its first.
    pub fn head(&self) -> Option<Seq> {
        self.head
    }

    /// Whether the event at `seq` was taken already: it stands at or below
    /// the head.
    pub(crate) fn has_taken(&self, seq: Seq) -> bool {
        self.head.is_some_and(|head| seq <= head)
    }

    /// Folds one event into every ledger its domain reaches.
    ///
    /// The order is fixed. The event is refused first, so a refused event
    /// moves nothing and books no time. A suspension the event states is
    /// recorded next, before the stretch it ends is booked, so that
    /// stretch leaves the sleep out. The stretch is then booked against
    /// what the ledgers hold open, and each owner folds the event. The
    /// activity counts it last.
    ///
    /// The dispatch names every domain: a kind this binary knows and no
    /// ledger reads would be a kind that silently derives nothing, which
    /// is the failure this shape exists to make impossible.
    fn apply(&mut self, event: &StoredEvent) -> Result<(), ReplayError> {
        self.refuse(event)?;
        if self.run_id.is_none() {
            self.run_id = Some(event.run_id.clone());
        }
        let meta = EventMeta::of(event);
        if let Some(EventPayload::Run(e @ RunEvent::HostSuspended(_))) = event.payload() {
            self.run.apply(e, &meta);
        }
        let open = Open::of(&self.run, &self.nodes, &self.gates, &self.tasks);
        self.clock.book(event, &open, self.run.suspensions());
        self.apply_owners(event, &meta)?;
        self.activity.apply(event);
        self.tail_paused = matches!(
            event.payload(),
            Some(EventPayload::Run(RunEvent::Paused(_)))
        );
        Ok(())
    }

    /// Hands the event to every ledger that reads its domain.
    fn apply_owners(
        &mut self,
        event: &StoredEvent,
        meta: &EventMeta<'_>,
    ) -> Result<(), ReplayError> {
        let Some(payload) = event.payload() else {
            // A kind this binary does not know: counted, named, never a
            // reason to stop deriving what it does know.
            self.unknown_events
                .push((event.seq, event.body.kind_name().to_string()));
            return Ok(());
        };
        match payload {
            EventPayload::Run(e) => self.apply_run(e, meta),
            EventPayload::Node(e) => {
                self.nodes.apply(e, meta, self.run.suspensions());
                self.tasks.apply_checks(e);
            }
            EventPayload::Session(e) => {
                self.nodes
                    .apply_session(e, meta)
                    .map_err(|_| ReplayError::SessionWithoutNode {
                        seq: event.seq,
                        kind: event.body.kind_name().to_string(),
                    })?;
                self.tasks.apply_session(e);
                self.degradations.apply(e, meta);
            }
            EventPayload::Tasks(e) => self.tasks.apply(e, meta).map_err(|UnknownTask(task)| {
                ReplayError::StatusWithoutTask {
                    seq: event.seq,
                    task,
                }
            })?,
            EventPayload::Scope(e) => self
                .grants
                .apply(e, meta)
                .map_err(|_| ReplayError::UngrantableSubject { seq: event.seq })?,
            EventPayload::Findings(e) => self.findings.apply(event.node_id.as_ref(), e),
            EventPayload::Artifacts(e) => {
                self.artifacts.apply(event.node_id.as_ref(), event.seq, e);
            }
            EventPayload::Gates(e) => {
                self.gate_transition(e, event, meta)?;
                self.gates.apply(e, meta);
            }
            EventPayload::Children(e) => self.children.apply(e, meta),
        }
        Ok(())
    }

    /// The run's own events. A host suspension was recorded before the clock
    /// booked its stretch, so it is not folded again here; a pause, a resume
    /// or a close ends every question the run still holds.
    fn apply_run(&mut self, event: &RunEvent, meta: &EventMeta<'_>) {
        if matches!(event, RunEvent::HostSuspended(_)) {
            return;
        }
        self.run.apply(event, meta);
        if matches!(
            event,
            RunEvent::Paused(_) | RunEvent::Resumed(_) | RunEvent::Finished(_)
        ) {
            self.gates.clear_asking();
        }
    }
}
