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
        let ledger = FindingLedger::of(&self.events().await?);
        match ledger.status(&answered.node, &answered.id) {
            Some(Slot::Live(_)) => {}
            Some(Slot::Withdrawn { reason }) => {
                return Err(RunToolError::WithdrawnFinding {
                    node: answered.node,
                    id: answered.id,
                    reason: reason.clone(),
                })
            }
            None => {
                return Err(RunToolError::NoSuchFinding {
                    node: answered.node,
                    id: answered.id,
                })
            }
        }
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
             beside the finding, which stands until its own node takes it back",
            answer.as_str()
        ))
    }
}
