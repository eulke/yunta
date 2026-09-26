//! How the `claude` CLI asks about a write, and what it reads back.
//!
//! The CLI runs a `PreToolUse` hook before every file-editing tool and
//! hands it the call as JSON on stdin. Exit 2 with the reason on stderr
//! is how that hook denies a call; exit 0 lets it through. The judgement
//! itself is [`yunta_core::fence::Fence::judge`] — this module only
//! translates.

use std::path::PathBuf;

use serde_json::Value;
use yunta_core::fence::{FenceHook, Verdict};
use yunta_core::port::{CodecError, FenceCodec, HookReply};

/// The tools this adapter's hook matches: every one that writes a file.
const WRITING_TOOLS: &str = "Edit|Write|MultiEdit|NotebookEdit";

/// How long the CLI waits for a judgement. A hook that does not answer
/// in time lets the call through — the post-check diff is what catches
/// that, and the session's coverage is what says it could.
const TIMEOUT_SECONDS: u32 = 10;

pub(super) struct ClaudeFenceCodec;

impl FenceCodec for ClaudeFenceCodec {
    fn decode(&self, stdin: &[u8]) -> Result<Option<PathBuf>, CodecError> {
        let call: Value = serde_json::from_slice(stdin)?;
        let input = call.get("tool_input").unwrap_or(&Value::Null);
        // `NotebookEdit` names its target `notebook_path`; every other
        // writing tool names it `file_path`.
        let path = ["file_path", "notebook_path"]
            .iter()
            .find_map(|field| input.get(*field).and_then(Value::as_str))
            .ok_or(CodecError::MissingField("tool_input.file_path"))?;
        Ok(Some(PathBuf::from(path)))
    }

    fn encode(&self, verdict: &Verdict) -> HookReply {
        match verdict {
            Verdict::Allowed => HookReply {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 0,
            },
            Verdict::Refused(refusal) => HookReply {
                stdout: Vec::new(),
                stderr: refusal.to_string().into_bytes(),
                exit: 2,
            },
        }
    }
}

/// The `--settings` document that installs the hook: the one place this
/// adapter states how its CLI is told to ask.
pub(super) fn settings_json(hook: &FenceHook) -> String {
    let command = hook
        .command(&super::ID)
        .iter()
        .map(|part| format!("{part:?}"))
        .collect::<Vec<_>>()
        .join(" ");
    serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": WRITING_TOOLS,
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "timeout": TIMEOUT_SECONDS,
                }],
            }],
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notebook_edit_names_its_target_by_its_own_field() {
        let call = br#"{"tool_name":"NotebookEdit","tool_input":{"notebook_path":"a.ipynb"}}"#;
        assert_eq!(
            ClaudeFenceCodec.decode(call).unwrap(),
            Some(PathBuf::from("a.ipynb"))
        );
    }

    #[test]
    fn a_call_with_no_path_at_all_is_a_call_this_codec_cannot_read() {
        let call = br#"{"tool_name":"Write","tool_input":{}}"#;
        assert!(matches!(
            ClaudeFenceCodec.decode(call),
            Err(CodecError::MissingField(_))
        ));
    }
}
