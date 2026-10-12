//! The samples of the gate kinds: an escalation waiting on a person, the
//! resolution that answered it, and the questions a node asked.

use crate::events::*;

pub(crate) fn gate_waiting() -> GateWaitingPayload {
    Escalation::published_to(
        "Ready to open the PR?",
        vec![Fact::labelled("published at", "example/repo#1")].into(),
        "https://github.com/example/repo/pull/1",
    )
    .expect("the summary states no fact the evidence holds")
    .into_payload()
}

pub(crate) fn gate_resolved() -> GateResolvedPayload {
    GateResolvedPayload::Approved {
        by: "eulke".into(),
        sha: "deadbeef".into(),
    }
}

pub(crate) fn questions_asked() -> QuestionsAskedPayload {
    QuestionsAskedPayload::new(
        crate::sha256_hex(b"sha256:222"),
        vec!["q1".into()],
        TokenUsage {
            input: 30,
            output: 12,
            cached: None,
        },
    )
    .expect("one question is a question")
}

pub(crate) fn questions_answered() -> QuestionsAnsweredPayload {
    QuestionsAnsweredPayload {
        answers_hash: crate::sha256_hex(b"sha256:333"),
        channel: Channel::Tty,
        responder: Some("eulke".into()),
    }
}

pub(crate) fn asking_opened() -> AskingOpenedPayload {
    AskingOpenedPayload {
        task_id: Some("T1".into()),
    }
}
