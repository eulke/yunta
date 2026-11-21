//! What stands in for the world beside a small run: a forge that publishes and
//! never answers, one whose rate limit is spent, and a person who answers a
//! round at the terminal of the engine that drives the run. And what a
//! workspace reading a small run reaches its forge through: the forge that
//! never answers, standing in for every forge a run's config declares
//! ([`StandInForge`]).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use yunta_api::ForgeSource;
use yunta_core::events::{Channel, GateWaitingPayload, HumanChoice};
use yunta_core::port::{
    Forge, ForgeError, ForgeProbe, ForgeUnanswered, HumanInteraction, PolledGate, PublishRequest,
    PullRequestRef, PullRequestRequest, QuestionsReply, ReviewOutcome,
};
use yunta_core::{Answer, ConfigLayer, NodeId, QuestionsFile, SecretSource};

/// The pull request a small run's forge opens, by its number.
const NUMBER: u64 = 7;

/// How long a spent rate limit asks to be waited out: the hour a forge's
/// limit resets on.
const RETRY_AFTER: Duration = Duration::from_secs(3_600);

/// The head a published pull request stands at, as the forge reports it: the
/// branch the gate pushed, which nobody moves while it waits.
const HEAD: &str = "0000000000000000000000000000000000000000";

/// A forge that publishes a gate's pull request and opens a run's, and whose
/// reviewers have not answered yet: how many times it was polled.
#[derive(Default)]
pub(super) struct Reviewing {
    polls: AtomicUsize,
}

/// Where a workspace reading the small runs reaches a forge: one forge whose reviewers
/// have not answered, standing in for every forge a run's config declares, and none for a
/// config that declares none, as a workspace's own source reaches none. A clone shares the
/// forge, and how many times it was polled. Not the forge itself, which it hands out.
#[derive(Clone, Default)]
pub struct StandInForge(Arc<Reviewing>);

impl StandInForge {
    /// How many times its forge was polled.
    pub fn polls(&self) -> usize {
        self.0.polls.load(Ordering::SeqCst)
    }
}

impl ForgeSource for StandInForge {
    fn forge(
        &self,
        config: &ConfigLayer,
        _secrets: &dyn SecretSource,
    ) -> Result<Arc<dyn Forge>, ForgeUnanswered> {
        let declared = config
            .forge
            .as_ref()
            .and_then(|forge| forge.github.as_ref());
        match declared {
            Some(_) => Ok(Arc::clone(&self.0) as Arc<dyn Forge>),
            None => Err(ForgeUnanswered::Undeclared),
        }
    }
}

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
        self.polls.fetch_add(1, Ordering::SeqCst);
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
