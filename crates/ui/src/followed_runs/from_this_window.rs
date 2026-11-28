//! See [`super`]. The runs this window handed on itself. A run handed to a detached engine is
//! kept as handed on, from the result of the work that handed it on (a start, a resume, a
//! decision or a round's answers recorded) until a reading of its engine says anything but
//! that its registry is not written: until then the engine it was handed to is about to
//! write it. A run this window started is kept on its way into the field until the field holds
//! it, so a field that lists no run in sight meanwhile is told from one with none.

use yunta_core::RunId;
use yunta_replay::registry::Engine;

use super::FollowedRuns;

impl FollowedRuns {
    /// `run`, handed from this window to a detached engine, started, resumed or answered,
    /// kept as handed on until a reading of its engine names more than an unwritten registry.
    pub(crate) fn handed_on(&mut self, run: RunId) {
        self.handed_on.insert(run);
        self.engines_read();
    }

    /// `run`, started from this window, on its way into the field until the field kept holds
    /// it.
    pub(crate) fn started(&mut self, run: RunId) {
        self.coming.insert(run);
        self.arrived();
    }

    /// Whether a run started from this window is yet to stand in the field kept: a field that
    /// lists no run in sight then is one about to list it.
    pub(crate) fn a_run_is_coming(&self) -> bool {
        !self.coming.is_empty()
    }

    /// The runs on their way into the field that the field kept holds now, let go.
    pub(super) fn arrived(&mut self) {
        if let Some(field) = &self.field {
            self.coming.retain(|run| !field.holds(run));
        }
    }

    /// Whether this window handed `run` to a detached engine whose registry no reading has
    /// shown written yet: such a run's unrecorded engine is one about to write it.
    pub(crate) fn handed_on_unread(&self, run: &RunId) -> bool {
        self.handed_on.contains(run)
    }

    /// The runs handed on here whose engine a reading kept here names, alive, dead or
    /// unknown, let go: from then on the reading alone says whether an engine drives it.
    pub(super) fn engines_read(&mut self) {
        let read = |run: &RunId| {
            let in_view = self.in_view.as_deref();
            let in_view = in_view.filter(|in_view| in_view.run.run() == run);
            let in_field = self.field.as_ref().and_then(|field| field.runs.get(run));
            let engines = in_view.map(|in_view| in_view.run.engine());
            let engines = engines.into_iter().chain(in_field.map(|run| run.engine()));
            engines
                .into_iter()
                .any(|engine| *engine != Engine::Unrecorded)
        };
        let handed_on = std::mem::take(&mut self.handed_on);
        let unread = handed_on.into_iter().filter(|run| !read(run)).collect();
        self.handed_on = unread;
    }
}
