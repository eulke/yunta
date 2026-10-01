//! Answering a finding another node reported: its work fixed it, or it
//! declines to, and why.
//!
//! An answer is what a node says about a finding, kept beside it — never
//! a change to it: the finding is still what was found, and only the
//! node that reported it updates it or takes it back. A call the engine
//! cannot honor is refused with what to fix and leaves the log as it was.

use serde::Deserialize;
use serde_json::Value;
use yunta_core::events::findings::{FindingLedger, Slot};
use yunta_core::events::{EventPayload, FindingAnswer, FindingAnsweredPayload, FindingEvent};
use yunta_core::{FindingId, NodeId};

use super::catalog::RunTool;
use super::session::{RunToolError, SessionTools};

/// What the session answers.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answered {
    node: NodeId,
    id: FindingId,
    answer: FindingAnswer,
    why: String,
}

impl SessionTools {
    /// The findings standing in the run, as the one view a gate shows.
    pub(super) async fn findings_standing(&self) -> Result<String, RunToolError> {
        let standing = FindingLedger::of(&self.events().await?).standing();
        serde_json::to_string_pretty(&standing).map_err(|source| RunToolError::Render { source })
    }

    pub(super) async fn answer_finding(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let answered: Answered = serde_json::from_value(Value::Object(args))
            .map_err(|source| RunToolError::InvalidAnswer { source })?;
        if answered.why.trim().is_empty() {
            return Err(RunToolError::EmptyAnswer);
        }
        if answered.node == self.node {
            return Err(RunToolError::OwnFinding {
                id: answered.id,
                update: self.called(RunTool::UpdateFinding),
                withdraw: self.called(RunTool::WithdrawFinding),
            });
        }
        answerable(
            &FindingLedger::of(&self.events().await?),
            &answered.node,
            &answered.id,
        )?;
        let (node, id, answer) = (answered.node.clone(), answered.id.clone(), answered.answer);
        self.append(EventPayload::Findings(FindingEvent::Answered(
            FindingAnsweredPayload {
                node: answered.node,
                id: answered.id,
                answer: answered.answer,
                why: answered.why,
            },
        )))
        .await?;
        Ok(format!(
            "your answer to `{node}`'s finding `{id}` — {} — is recorded: a person reads it \
             beside the finding, which stands until its own node takes it back. Answered \
             fixed, the criterion it proposes runs on the tree you leave, and passing \
             settles it",
            answer.as_str()
        ))
    }
}

/// Whether the finding `id` that `node` reported can be answered: it
/// stands, and nothing settled it.
fn answerable(ledger: &FindingLedger, node: &NodeId, id: &FindingId) -> Result<(), RunToolError> {
    if let Some(settled) = ledger.settled(Some(node), id) {
        return Err(RunToolError::SettledFinding {
            node: node.clone(),
            id: id.clone(),
            settled: settled_by(settled),
        });
    }
    match ledger.status(node, id) {
        Some(Slot::Live(_)) => Ok(()),
        Some(Slot::Withdrawn { reason }) => Err(RunToolError::WithdrawnFinding {
            node: node.clone(),
            id: id.clone(),
            reason: reason.clone(),
        }),
        None => Err(RunToolError::NoSuchFinding {
            node: node.clone(),
            id: id.clone(),
        }),
    }
}

/// What settled a finding, as a refusal names it.
fn settled_by(settled: &yunta_core::events::findings::Settled) -> String {
    match settled {
        yunta_core::events::findings::Settled::Proof { cmd } => format!("`{cmd}` passed"),
    }
}
