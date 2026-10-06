//! How the `claude` CLI's hooks ask the engine, and what they read back.
//!
//! The CLI runs a `PreToolUse` hook before every file-editing tool and
//! hands it the call as JSON on stdin. Exit 2 with the reason on stderr
//! is how that hook denies a call; exit 0 lets it through. What the
//! engine answers is the engine's — this module only translates.

use std::path::PathBuf;

use serde_json::Value;
use yunta_core::port::{CodecError, HookReply, RailCodec};
use yunta_core::rail::{RailAct, RailAnswer, RailCall, RailCommand, RailEvent};
use yunta_core::SessionId;

/// The tools this adapter's write hook matches: every one that writes a
/// file.
const WRITING_TOOLS: &str = "Edit|Write|MultiEdit|NotebookEdit";

pub(super) struct ClaudeRailCodec;

impl RailCodec for ClaudeRailCodec {
    fn decode(&self, event: RailEvent, stdin: &[u8]) -> Result<Option<RailCall>, CodecError> {
        let call: Value = serde_json::from_slice(stdin)?;
        let session_id = call
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<SessionId>().ok())
            .ok_or(CodecError::MissingField("session_id"))?;
        match event {
            RailEvent::BeforeWrite => {
                let input = call.get("tool_input").unwrap_or(&Value::Null);
                // `NotebookEdit` names its target `notebook_path`; every
                // other writing tool names it `file_path`.
                let path = ["file_path", "notebook_path"]
                    .iter()
                    .find_map(|field| input.get(*field).and_then(Value::as_str))
                    .ok_or(CodecError::MissingField("tool_input.file_path"))?;
                Ok(Some(RailCall {
                    session_id,
                    act: RailAct::BeforeWrite {
                        paths: vec![PathBuf::from(path)],
                    },
                }))
            }
        }
    }

    fn encode(&self, event: RailEvent, answer: &RailAnswer) -> HookReply {
        match (event, answer) {
            (RailEvent::BeforeWrite, RailAnswer::Refuse { reason }) => HookReply {
                stdout: Vec::new(),
                stderr: reason.clone().into_bytes(),
                exit: 2,
            },
            (RailEvent::BeforeWrite, RailAnswer::Proceed) => HookReply {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 0,
            },
        }
    }
}

/// The `--settings` document that installs the hooks: the one place this
/// adapter states how its CLI is told to ask.
pub(super) fn settings_json(command: &RailCommand) -> String {
    let hook = |event: RailEvent| {
        command
            .argv(&super::ID, event)
            .iter()
            .map(|part| crate::session::sh_quoted(part))
            .collect::<Vec<_>>()
            .join(" ")
    };
    serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": WRITING_TOOLS,
                "hooks": [{
                    "type": "command",
                    "command": hook(RailEvent::BeforeWrite),
                    "timeout": crate::session::HOOK_TIMEOUT_SECONDS,
                }],
            }],
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(call: &[u8]) -> Result<Option<RailCall>, CodecError> {
        ClaudeRailCodec.decode(RailEvent::BeforeWrite, call)
    }

    #[test]
    fn a_notebook_edit_names_its_target_by_its_own_field() {
        let call = br#"{"session_id":"s-1","tool_name":"NotebookEdit","tool_input":{"notebook_path":"a.ipynb"}}"#;
        assert_eq!(
            decoded(call).unwrap().map(|call| call.act),
            Some(RailAct::BeforeWrite {
                paths: vec![PathBuf::from("a.ipynb")]
            })
        );
    }

    #[test]
    fn a_call_with_no_path_at_all_is_a_call_this_codec_cannot_read() {
        let call = br#"{"session_id":"s-1","tool_name":"Write","tool_input":{}}"#;
        assert!(matches!(decoded(call), Err(CodecError::MissingField(_))));
    }

    #[test]
    fn a_call_that_names_no_session_is_a_call_this_codec_cannot_read() {
        let call = br#"{"tool_name":"Write","tool_input":{"file_path":"a.rs"}}"#;
        assert!(matches!(
            decoded(call),
            Err(CodecError::MissingField("session_id"))
        ));
    }

    #[test]
    fn a_refusal_is_exit_two_with_the_reason_on_stderr() {
        let reply = ClaudeRailCodec.encode(
            RailEvent::BeforeWrite,
            &RailAnswer::Refuse {
                reason: "no".to_string(),
            },
        );
        assert_eq!((reply.exit, reply.stderr), (2, b"no".to_vec()));
    }
}
