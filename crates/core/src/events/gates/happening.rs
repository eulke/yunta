//! What a gate event says happened, read as a person reads it.

use crate::events::{Channel, GateEvent, GateResolvedPayload, GateWaitingPayload};
use crate::{QuestionId, Responder};

/// One thing that happened at a decision or a round of questions.
#[derive(Debug, Clone, PartialEq)]
pub enum Happening {
    Escalated(Box<GateWaitingPayload>),
    Resolved(Box<GateResolvedPayload>),
    /// A node that asked, and what it asked — so a reader knows what the
    /// run is waiting on without opening the document.
    Asked {
        questions: Vec<QuestionId>,
    },
    Answered {
        channel: Channel,
        responder: Option<Responder>,
    },
}

impl From<&GateEvent> for Happening {
    fn from(event: &GateEvent) -> Self {
        match event {
            GateEvent::Waiting(p) => Happening::Escalated(Box::new(p.clone())),
            GateEvent::Resolved(p) => Happening::Resolved(Box::new(p.clone())),
            GateEvent::QuestionsAsked(p) => Happening::Asked {
                questions: p.questions.clone(),
            },
            GateEvent::QuestionsAnswered(p) => Happening::Answered {
                channel: p.channel,
                responder: p.responder.clone(),
            },
        }
    }
}
