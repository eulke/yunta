//! The one place the engine asks whether an adapter can do something.
//!
//! [`require`] reads [`POLICY`] and answers: granted, degraded under a
//! stated fallback, or refused. Every consultation goes through it, so a
//! capability can never be consulted in one place and forgotten in
//! another, and the sentence a degradation records is the same wherever
//! it happens.

use yunta_core::port::{absence_of, Absence, Adapter};
use yunta_core::{Capability, Node};

use super::{RunCtx, RunError};

/// What [`require`] answers. `Degraded` is already recorded on the log
/// when it is returned: the caller reads it to decide what to hand the
/// session, never to decide whether to say something.
pub(crate) enum Decision {
    /// The adapter declares it.
    Granted,
    /// It does not, and the run goes on under the fallback the log now
    /// carries. What that fallback was is on the log, once, so nothing
    /// downstream has to be told it a second time.
    Degraded,
    /// It does not, and what the node declared has no other way to
    /// happen.
    Refused(RunError),
}

/// Whether `adapter` can do `capability` for `node`, and what happens
/// when it cannot.
///
/// A degradation is written here, once, before the answer comes back —
/// so a caller cannot take the fallback and forget to say so. A fallback
/// the whole run works under (`usage_reporting`, `edit_hooks`) is stated
/// once per run rather than once per node: the second event says nothing
/// the first did not.
pub(crate) async fn require(
    ctx: &RunCtx<'_>,
    adapter: &dyn Adapter,
    capability: Capability,
    node: &Node,
) -> Result<Decision, RunError> {
    if adapter.capabilities().declares(capability) {
        return Ok(Decision::Granted);
    }
    match absence_of(capability) {
        Absence::Resting => Ok(Decision::Granted),
        // `check` refused this workflow before the run was born, so a
        // run reaching here was born under an older binary or a
        // hand-edited manifest. It states the same refusal the check
        // would have.
        Absence::FailAtCheck | Absence::FailNode => Ok(Decision::Refused(RunError::Broken {
            diagnostic: format!(
                "node `{}` needs `{capability}` and adapter `{}` declares none — \
                 pick a runner on an adapter that has it",
                node.id,
                adapter.id()
            ),
        })),
        Absence::DegradeWith(policy) => {
            if !stated_once_per_run(capability) || !already_stated(ctx, capability).await? {
                ctx.emit(
                    Some(&node.id),
                    yunta_core::events::EventPayload::Session(
                        yunta_core::events::SessionEvent::CapabilityDegraded(
                            yunta_core::events::CapabilityDegradedPayload::new(
                                capability,
                                adapter.id().clone(),
                                *policy,
                            ),
                        ),
                    ),
                )
                .await?;
            }
            Ok(Decision::Degraded)
        }
    }
}

/// The capabilities whose absence is the run's condition rather than a
/// node's choice: nothing a node declares turns them on, so the log
/// states them once and every node after reads that one event.
fn stated_once_per_run(capability: Capability) -> bool {
    matches!(
        capability,
        Capability::UsageReporting | Capability::EditHooks
    )
}

/// Whether this run's log already carries a degradation of `capability`.
async fn already_stated(ctx: &RunCtx<'_>, capability: Capability) -> Result<bool, RunError> {
    Ok(ctx
        .run_view()
        .await?
        .state
        .degradations
        .already_stated(capability))
}
