//! A step the run carries on past when it fails. The failure is data, so the
//! caller records it on the log instead of dropping it.

use serde::{Deserialize, Serialize};

/// The closed set of best-effort steps the log names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BestEffortStep {
    /// Closing the MCP client after its one query.
    McpClientClose,
}

/// A step that did not complete; the source's message is kept verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortfall {
    pub step: BestEffortStep,
    pub cause: String,
}

/// The failure of a best-effort step, kept as data for the caller to record.
#[must_use = "a shortfall is recorded on the log, never dropped"]
pub fn record<E: std::fmt::Display>(step: BestEffortStep, error: E) -> Shortfall {
    Shortfall {
        step,
        cause: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_keeps_each_error_message_verbatim() {
        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let join = task.await.expect_err("an aborted task joins with an error");
        let join_shortfall = record(BestEffortStep::McpClientClose, join);
        assert_eq!(join_shortfall.step, BestEffortStep::McpClientClose);
        assert!(
            join_shortfall.cause.ends_with("was cancelled"),
            "{}",
            join_shortfall.cause
        );

        let io_shortfall = record(
            BestEffortStep::McpClientClose,
            std::io::Error::other("the pipe closed"),
        );
        assert_eq!(
            io_shortfall,
            Shortfall {
                step: BestEffortStep::McpClientClose,
                cause: "the pipe closed".to_string(),
            }
        );
    }
}
