//! What a closed run reports about its spend: the tokens it used, and what
//! each task it verified cost.

use serde::{Deserialize, Serialize};

use crate::events::session::payloads::TokenUsage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cptv: Option<f64>,
    pub tokens: TokenUsage,
}

impl RunMetrics {
    /// The metrics of a run that spent `tokens` and verified `tasks_done`
    /// tasks, the ones that reached `done`. Cost per verified task is the
    /// total spend over those tasks, and a run that verified none has no such
    /// cost: `None`, never a zero that reads like a free run.
    pub fn of(tokens: TokenUsage, tasks_done: usize) -> RunMetrics {
        RunMetrics {
            cptv: (tasks_done > 0).then(|| tokens.total() as f64 / tasks_done as f64),
            tokens,
        }
    }
}
