//! Asking a person at the engine's terminal: what the run says while it
//! asks, and the answer it takes back.

use yunta_core::events::{
    AskingOpenedPayload, EventPayload, GateEvent, GateWaitingPayload, HumanChoice,
};
use yunta_core::{NodeId, TaskId};

use super::{RunCtx, RunError};

impl RunCtx<'_> {
    /// Puts `escalation` to the run's human surface, with the documents
    /// it shows, and returns its choice,
    /// verified against the menu the surface was shown. `None` keeps its
    /// meaning: no surface can answer right now. An answer the
    /// escalation does not accept is refused as
    /// [`RunError::RefusedAnswer`] before anything is recorded. While the
    /// surface asks, the run's registry says `node` is asking, so a
    /// reader elsewhere knows the run needs a person.
    pub(crate) async fn ask_human(
        &self,
        node: Option<&NodeId>,
        task: Option<&TaskId>,
        escalation: &GateWaitingPayload,
    ) -> Result<Option<HumanChoice>, RunError> {
        // A plan is shown as the run judges it — with what a person
        // accepted departing from it and the suite the run measured,
        // which only the log holds.
        let shown = match escalation.shows().is_empty() {
            true => Vec::new(),
            false => {
                let state = self.run_view().await?.state;
                crate::artifacts::shown::documents(self.run_dir, escalation.shows(), &state).await?
            }
        };
        let asking = crate::Asking {
            shown: &shown,
            run: self.run_id,
        };
        let answered = {
            let _asking = self.asking(node, task).await?;
            self.human_interaction.resolve_in(escalation, &asking).await
        };
        let Some(choice) = answered else {
            return Ok(None);
        };
        escalation
            .accepts(&choice)
            .map_err(|refused| RunError::RefusedAnswer {
                refused,
                summary: escalation.summary().to_string(),
            })?;
        Ok(Some(choice))
    }

    /// Says in the run's registry that `node` is asking a person at this
    /// engine's terminal, until what this returns is dropped — however
    /// the asking ends — and, when a person is there to be asked, says on
    /// the log when the asking began: the answer lands only once it is
    /// given, and the wait before it is the person's.
    pub(crate) async fn asking(
        &self,
        node: Option<&NodeId>,
        task: Option<&TaskId>,
    ) -> Result<Asking<'_>, RunError> {
        if self.human_interaction.present() {
            self.emit(
                node,
                EventPayload::Gates(GateEvent::AskingOpened(AskingOpenedPayload {
                    task_id: task.cloned(),
                })),
            )
            .await?;
        }
        if let Some(registry) = &self.process_registry {
            registry.asking(Some(crate::process_registry::Asked {
                node: node.cloned(),
                since: self.clock.now(),
            }));
        }
        Ok(Asking {
            registry: self.process_registry.as_deref(),
        })
    }
}

/// A question this engine is asking a person at its terminal, recorded
/// in the run's registry until it is dropped.
pub(crate) struct Asking<'r> {
    registry: Option<&'r crate::process_registry::ProcessRegistry>,
}

impl Drop for Asking<'_> {
    fn drop(&mut self) {
        if let Some(registry) = self.registry {
            registry.asking(None);
        }
    }
}
