//! What stands in for the world beside a small run: a forge that publishes and
//! never answers, one whose rate limit is spent, and a person who answers a
//! round at the terminal of the engine that drives the run.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use yunta_core::events::{Channel, GateWaitingPayload, HumanChoice};
use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, HumanInteraction, PolledGate, PublishRequest, PullRequestRef,
    PullRequestRequest, QuestionsReply, ReviewOutcome,
};
use yunta_core::{Answer, NodeId, QuestionsFile};

/// The pull request a small run's forge opens, by its number.
const NUMBER: u64 = 7;

/// How long a spent rate limit asks to be waited out: the hour a forge's
/// limit resets on.
const RETRY_AFTER: Duration = Duration::from_secs(3_600);

/// The head a published pull request stands at, as the forge reports it: the
/// branch the gate pushed, which nobody moves while it waits.
const HEAD: &str = "0000000000000000000000000000000000000000";

/// A forge that publishes a gate's pull request and opens a run's, and whose
/// reviewers have not answered yet.
pub(super) struct Reviewing;

/// A forge whose rate limit is spent: it refuses to open a pull request until
/// its reset, an hour on.
pub(super) struct Limited;

/// The pull request a small run's forge opens.
fn pull_request() -> PullRequestRef {
    PullRequestRef {
        url: format!("https://github.com/acme/web/pull/{NUMBER}"),
        number: NUMBER,
    }
}

#[async_trait]
impl Forge for Reviewing {
    async fn publish(&self, _request: &PublishRequest) -> Result<PullRequestRef, ForgeError> {
        Ok(pull_request())
    }

    async fn poll(&self, _gate: &PullRequestRef) -> Result<PolledGate, ForgeError> {
        Ok(PolledGate {
            head_sha: HEAD.parse().expect("a commit's sha"),
            review: ReviewOutcome::Pending,
        })
    }

    async fn open_pull_request(
        &self,
        _request: &PullRequestRequest,
    ) -> Result<PullRequestRef, ForgeError> {
        Ok(pull_request())
    }

    async fn probe(&self) -> Result<ForgeProbe, ForgeError> {
        Ok(ForgeProbe {
            can_push: Some(true),
        })
    }
}

#[async_trait]
impl Forge for Limited {
    async fn publish(&self, _request: &PublishRequest) -> Result<PullRequestRef, ForgeError> {
        Err(limited("publish a gate"))
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
        Err(limited("open a pull request"))
    }

    async fn probe(&self) -> Result<ForgeProbe, ForgeError> {
        Ok(ForgeProbe {
            can_push: Some(true),
        })
    }
}

/// The refusal of a forge whose rate limit is spent, while it does `action`.
fn limited(action: &'static str) -> ForgeError {
    ForgeError::RateLimited {
        action,
        retry_after: Some(RETRY_AFTER),
    }
}

/// The forge `forge` stands for, as a bench takes one.
pub(super) fn forge(forge: impl Forge + 'static) -> Arc<dyn Forge> {
    Arc::new(forge)
}

/// A person at the engine's own terminal who answers each round with
/// `answers`, each a question's id and its value, and each decision with
/// `choice`, when there is one.
pub(super) struct AtTheTerminal {
    pub(super) answers: &'static [(&'static str, &'static str)],
    pub(super) choice: Option<&'static str>,
}

#[async_trait]
impl HumanInteraction for AtTheTerminal {
    fn present(&self) -> bool {
        true
    }

    async fn resolve(&self, _escalation: &GateWaitingPayload) -> Option<HumanChoice> {
        self.choice.map(|option| HumanChoice {
            option: option.into(),
            by: "lead".into(),
            free_text: None,
        })
    }

    async fn ask(&self, _node: &NodeId, _questions: &QuestionsFile) -> Option<QuestionsReply> {
        Some(QuestionsReply {
            answers: self
                .answers
                .iter()
                .map(|(id, value)| Answer {
                    id: (*id).into(),
                    value: (*value).to_string(),
                })
                .collect(),
            channel: Channel::Tty,
            responder: Some("lead".into()),
        })
    }
}
