//! Answering a run a [`Bench`] parked, as a process that never ran it
//! answers: the decision its gate waits on, or the round of questions one of
//! its nodes asked, written on its log through the engine's own door for
//! each and stamped by the bench's clock.
//!
//! Nothing here wakes the run. What an answer leads to is the next wake's,
//! as the detached resume a surface hands the run to applies it, so a test
//! that records a run parks it, answers it here and wakes it.

use yunta_core::events::{Channel, HumanChoice};
use yunta_core::{Answer, Manifest, NodeId, Redactor, Responder};

use super::{no_secrets, Bench};

impl Bench {
    /// Records `option`, with the words `said` a person gives with it, on the
    /// decision this bench's run is parked on: the write `yunta resolve-gate`
    /// makes, attributed to `mcp`. Refused, with nothing written, where the run
    /// is not parked or its decision does not take that answer.
    pub async fn answer_parked(
        &self,
        option: &str,
        said: Option<&str>,
    ) -> Result<(), yunta_engine::ResolveGateError> {
        let manifest = self.manifest();
        yunta_engine::resolve_gate(
            &manifest,
            &self.run_dir(),
            &self.storage.async_handle(),
            &self.run_id,
            self.clock.as_ref(),
            HumanChoice {
                option: option.into(),
                by: "mcp".into(),
                free_text: said.map(str::to_string),
            },
            &redactor(&manifest),
        )
        .await
    }

    /// Records `answers`, each a question's id and its value, as the reply to
    /// the round of questions `node` asked and this bench's run is parked on:
    /// the write of a person who answers it at a terminal of another process,
    /// on the `tty` channel and attributed to the bench's user, unverified.
    /// Refused, with nothing written, where the reply does not answer the
    /// round.
    pub async fn answer_round(
        &self,
        node: &str,
        answers: &[(&str, &str)],
    ) -> Result<yunta_engine::AnswersRecorded, yunta_engine::AnswerQuestionsError> {
        let user = self.ambient.as_ref().and_then(|env| env.user.as_deref());
        let reply = yunta_engine::AnswersReply {
            answers: answers
                .iter()
                .map(|(id, value)| Answer {
                    id: (*id).into(),
                    value: (*value).to_string(),
                })
                .collect(),
            channel: Channel::Tty,
            responder: Some(Responder::answering(None, user)),
        };
        yunta_engine::answer_questions(
            &redactor(&self.manifest()),
            &self.storage.async_handle(),
            &self.run_id,
            &self.run_dir(),
            self.clock.as_ref(),
            &NodeId::from(node),
            reply,
        )
        .await
    }
}

/// What stands between an answer a person typed and the log of a run of
/// `manifest`: the run's own redactor, over a bench's secrets.
fn redactor(manifest: &Manifest) -> Redactor {
    yunta_engine::run_redactor(manifest, &*no_secrets())
        .expect("the bench's config names no secret too short to keep out of a log")
}
