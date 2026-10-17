//! What the engine answers a hook, at each moment a session asks.
//!
//! Each moment has one typed answer. The engine decides it, and the
//! adapter's codec writes it in the shape its CLI reads. A write refusal
//! is the text a model reads as a refusal, so it always leads with the
//! fence's marker and always names its reason.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fence::Refusal;
use crate::port::session::{HookReply, RailCodec};
use crate::rail::RailEvent;
use crate::{CommandName, ToolNaming};

/// Leads every write refusal; the fence module re-exports it.
pub const REFUSAL_MARKER: &str = "yunta: write refused: ";

/// Never empty, always led by the marker; deserializing runs the same check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WriteRefusal(String);

/// A text that cannot be a write refusal: no marker, or no reason after it.
#[derive(Debug, Error)]
#[error("a write refusal must lead with the marker and name its reason")]
pub struct RefusalUnread;

impl TryFrom<String> for WriteRefusal {
    type Error = RefusalUnread;

    fn try_from(text: String) -> Result<Self, RefusalUnread> {
        let read = text
            .strip_prefix(REFUSAL_MARKER)
            .is_some_and(|rest| !rest.trim().is_empty());
        if read {
            Ok(Self(text))
        } else {
            Err(RefusalUnread)
        }
    }
}

impl From<WriteRefusal> for String {
    fn from(refusal: WriteRefusal) -> String {
        refusal.0
    }
}

impl WriteRefusal {
    /// The fence's texts, joined; `None` when there is none to say. Each
    /// fence text leads with the marker, so the join does too.
    pub fn of(refusals: &[Refusal], naming: ToolNaming) -> Option<Self> {
        if refusals.is_empty() {
            return None;
        }
        let text = refusals
            .iter()
            .map(|refusal| refusal.message(naming))
            .collect::<Vec<_>>()
            .join("\n\n");
        Some(Self(text))
    }

    /// The marker and the reason the engine could not be asked.
    pub fn unasked(why: &str) -> Self {
        let reason = format!("{REFUSAL_MARKER}the engine could not be asked ({why}).");
        Self(reason)
    }
}

/// What the engine answers a write before it happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum WriteAnswer {
    /// Go on, and write.
    Proceed,
    /// Do not write; `reason` is what the model reads.
    Refuse { reason: WriteRefusal },
}

/// What the engine answers after a write, once its formatters have run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum FormatAnswer {
    /// Go on: no formatter had anything the model must read.
    Proceed,
    /// Go on, and know this: `notes` is what the model reads.
    Tell { notes: Vec<FormatNote> },
}

/// What the engine answers when a session is about to stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum StopAnswer {
    /// Let the session stop.
    Proceed,
    /// Keep working; `reason` is what the model reads.
    Refuse { reason: String },
}

/// Data, not prose: `Display` in `message.rs` writes what the model reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "note", rename_all = "snake_case")]
pub enum FormatNote {
    /// The formatter changed these files.
    Changed {
        run: CommandName,
        paths: Vec<PathBuf>,
    },
    /// The formatter exited with `exit_code`; `printed` holds the last
    /// lines it wrote, redacted.
    Failed {
        run: CommandName,
        exit_code: i32,
        printed: Vec<String>,
    },
    /// The formatter ran past its bound and was stopped after `after_ms`.
    TimedOut { run: CommandName, after_ms: u64 },
}

/// One moment's answer: what a hook answers when it could not ask, and
/// what the CLI's codec writes back for the answer.
pub trait HookAnswer: Serialize + DeserializeOwned + Sized {
    /// The moment this answer belongs to.
    const MOMENT: RailEvent;
    /// What a hook answers when it could not ask: a write refuses, the others proceed.
    fn unasked(why: &str) -> Self;
    /// The bytes this CLI reads for this answer at this moment.
    fn reply(&self, codec: &dyn RailCodec) -> HookReply;
}

impl HookAnswer for WriteAnswer {
    const MOMENT: RailEvent = RailEvent::BeforeWrite;

    fn unasked(why: &str) -> Self {
        WriteAnswer::Refuse {
            reason: WriteRefusal::unasked(why),
        }
    }

    fn reply(&self, codec: &dyn RailCodec) -> HookReply {
        codec.before_write(self)
    }
}

impl HookAnswer for FormatAnswer {
    const MOMENT: RailEvent = RailEvent::AfterWrite;

    fn unasked(_why: &str) -> Self {
        FormatAnswer::Proceed
    }

    fn reply(&self, codec: &dyn RailCodec) -> HookReply {
        codec.after_write(self)
    }
}

impl HookAnswer for StopAnswer {
    const MOMENT: RailEvent = RailEvent::BeforeStop;

    fn unasked(_why: &str) -> Self {
        StopAnswer::Proceed
    }

    fn reply(&self, codec: &dyn RailCodec) -> HookReply {
        codec.before_stop(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fence::Advice;

    // Guard: pins an invariant the new type enforces.
    #[test]
    fn an_empty_refusal_does_not_read() {
        assert!(WriteRefusal::try_from(String::new()).is_err());
        assert!(WriteRefusal::try_from("a reason without the marker".to_string()).is_err());
        assert!(serde_json::from_str::<WriteAnswer>(
            r#"{"answer":"refuse","reason":"yunta: write refused: "}"#
        )
        .is_err());
    }

    #[test]
    fn a_hook_that_could_not_ask_refuses_a_write() {
        assert!(matches!(
            WriteAnswer::unasked("down"),
            WriteAnswer::Refuse { .. }
        ));
    }

    #[test]
    fn an_unasked_write_refuses_with_the_marker_and_the_reason() {
        let wire = serde_json::to_value(WriteAnswer::unasked("down")).unwrap();
        assert_eq!(wire["answer"], "refuse");
        assert_eq!(
            wire["reason"],
            "yunta: write refused: the engine could not be asked (down)."
        );
        let proceed = serde_json::to_value(WriteAnswer::Proceed).unwrap();
        assert_eq!(proceed["answer"], "proceed");
    }

    #[test]
    fn the_refusals_of_a_fence_join_under_the_marker() {
        let refusal = Refusal {
            target: PathBuf::from("/work/a.txt"),
            worktree: PathBuf::from("/work"),
            allowed: Some(Vec::new()),
            roots: Vec::new(),
            advice: Advice::ReportFinding,
            protection: None,
        };
        let joined = WriteRefusal::of(&[refusal.clone(), refusal], ToolNaming::Bare).unwrap();
        let text: String = joined.clone().into();
        assert!(text.starts_with(REFUSAL_MARKER), "{text}");
        assert_eq!(WriteRefusal::try_from(text).unwrap(), joined);
        assert!(WriteRefusal::of(&[], ToolNaming::Bare).is_none());
    }
}
