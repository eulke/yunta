//! The forge the design's run opens its pull request on: it answers every
//! request with the one pull request the run opens, on the repository the run's
//! config names, and publishes no gate, since the run's gates are a person's.

use async_trait::async_trait;
use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, PolledGate, PublishRequest, PullRequestRef, PullRequestRequest,
};

/// The number of the pull request the run opens: the one after the last the
/// repository it names had opened when the design was drawn.
const NUMBER: u64 = 31;

/// The forge of the repository the design's run works in.
pub(super) struct DesignedForge;

#[async_trait]
impl Forge for DesignedForge {
    async fn publish(&self, request: &PublishRequest) -> Result<PullRequestRef, ForgeError> {
        Err(ForgeError::Http {
            action: "publish a gate",
            status: 404,
            body: format!("the design's run publishes no gate: {}", request.branch),
        })
    }

    async fn poll(&self, gate: &PullRequestRef) -> Result<PolledGate, ForgeError> {
        Err(ForgeError::UnknownGate {
            number: gate.number,
        })
    }

    async fn open_pull_request(
        &self,
        _request: &PullRequestRequest,
    ) -> Result<PullRequestRef, ForgeError> {
        Ok(PullRequestRef {
            url: format!("https://github.com/eulke/yunta/pull/{NUMBER}"),
            number: NUMBER,
        })
    }

    async fn probe(&self) -> Result<ForgeProbe, ForgeError> {
        Ok(ForgeProbe {
            can_push: Some(true),
        })
    }
}
