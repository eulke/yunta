//! How the `claude` CLI's hooks ask the engine, and what they read back.
//!
//! The CLI runs a `PreToolUse` hook before every file-editing tool and
//! hands it the call as JSON on stdin. Exit 2 with the reason on stderr
//! is how that hook denies a call; exit 0 lets it through. A `PostToolUse`
//! hook after the same tools hands the model `additionalContext` when the
//! engine has something to say about what was written. Its `Stop` hook
//! runs when the session would end its turn: `{"decision":"block"}` with
//! a reason on stdout keeps it working, and the model reads the reason.
//! What the engine answers is the engine's — this module only translates.

use std::path::PathBuf;

use serde_json::Value;
use yunta_core::port::{CodecError, HookReply, RailCodec};
use yunta_core::rail::{RailAct, RailAnswer, RailCall, RailCommand, RailEvent};
use yunta_core::SessionId;

/// The tools this adapter's write hook matches: every one that writes a
/// file.
const WRITING_TOOLS: &str = "Edit|Write|MultiEdit|NotebookEdit";

pub(crate) struct ClaudeRailCodec;

impl RailCodec for ClaudeRailCodec {
    fn decode(&self, event: RailEvent, stdin: &[u8]) -> Result<Option<RailCall>, CodecError> {
        let call: Value = serde_json::from_slice(stdin)?;
        let session_id = call
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<SessionId>().ok())
            .ok_or(CodecError::MissingField("session_id"))?;
        match event {
            RailEvent::BeforeStop => Ok(Some(RailCall {
                session_id,
                act: RailAct::BeforeStop,
            })),
            RailEvent::BeforeWrite | RailEvent::AfterWrite => {
                let input = call.get("tool_input").unwrap_or(&Value::Null);
                // `NotebookEdit` names its target `notebook_path`; every
                // other writing tool names it `file_path`.
                let path = ["file_path", "notebook_path"]
                    .iter()
                    .find_map(|field| input.get(*field).and_then(Value::as_str))
                    .ok_or(CodecError::MissingField("tool_input.file_path"))?;
                let paths = vec![PathBuf::from(path)];
                let act = match event {
                    RailEvent::AfterWrite => RailAct::AfterWrite { paths },
                    _ => RailAct::BeforeWrite { paths },
                };
                Ok(Some(RailCall { session_id, act }))
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
            (RailEvent::BeforeStop, RailAnswer::Refuse { reason }) => {
                crate::session::kept_working(reason)
            }
            (
                RailEvent::AfterWrite,
                RailAnswer::Tell { context: said } | RailAnswer::Refuse { reason: said },
            ) => crate::session::told_after_write(said),
            (_, RailAnswer::Proceed | RailAnswer::Tell { .. }) => HookReply {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: 0,
            },
        }
    }
}

/// The answer the `claude` CLI takes from a hook's reply: exit 2 denies a
/// write with its stderr shown to the model, and anything else goes on.
pub(crate) fn read_reply(event: RailEvent, reply: &HookReply) -> RailAnswer {
    match event {
        RailEvent::BeforeWrite if reply.exit == 2 => RailAnswer::Refuse {
            reason: String::from_utf8_lossy(&reply.stderr).into_owned(),
        },
        RailEvent::BeforeWrite => RailAnswer::Proceed,
        RailEvent::BeforeStop => crate::session::stop_answer(reply),
        RailEvent::AfterWrite => crate::session::after_write_answer(reply),
    }
}

/// The `--settings` document that installs the hooks — the one place this
/// adapter states how its CLI is told to ask — beside the `sandbox` its
/// commands run in, when they are confined. The stop hook waits for the
/// engine as long as `judging` allows: judging the work runs its checks.
/// The hook after a write is installed only when the engine `formats` it.
pub(crate) fn settings_json(
    command: &RailCommand,
    sandbox: Option<serde_json::Value>,
    judging: std::time::Duration,
    formats: bool,
) -> String {
    let hook = |event: RailEvent| {
        command
            .argv(&super::ID, event)
            .iter()
            .map(|part| crate::session::sh_quoted(part))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut hooks = serde_json::Map::from_iter([
        (
            "PreToolUse".to_string(),
            serde_json::json!([{
                "matcher": WRITING_TOOLS,
                "hooks": [{
                    "type": "command",
                    "command": hook(RailEvent::BeforeWrite),
                    "timeout": crate::session::HOOK_TIMEOUT_SECONDS,
                }],
            }]),
        ),
        (
            "Stop".to_string(),
            serde_json::json!([{
                "hooks": [{
                    "type": "command",
                    "command": hook(RailEvent::BeforeStop),
                    "timeout": judging.as_secs(),
                }],
            }]),
        ),
    ]);
    if formats {
        hooks.insert(
            "PostToolUse".to_string(),
            serde_json::json!([{
                "matcher": WRITING_TOOLS,
                "hooks": [{
                    "type": "command",
                    "command": hook(RailEvent::AfterWrite),
                    "timeout": crate::session::HOOK_TIMEOUT_SECONDS,
                }],
            }]),
        );
    }
    let mut settings = serde_json::Map::new();
    settings.insert("hooks".to_string(), serde_json::Value::Object(hooks));
    if let Some(sandbox) = sandbox {
        settings.insert("sandbox".to_string(), sandbox);
    }
    serde_json::Value::Object(settings).to_string()
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
    fn a_refused_stop_keeps_the_session_working_with_the_reason() {
        let reply = ClaudeRailCodec.encode(
            RailEvent::BeforeStop,
            &RailAnswer::Refuse {
                reason: "a criterion is red".to_string(),
            },
        );
        let said: Value = serde_json::from_slice(&reply.stdout).unwrap();
        assert_eq!(reply.exit, 0);
        assert_eq!(said["decision"], "block");
        assert_eq!(said["reason"], "a criterion is red");
        let call = br#"{"session_id":"s-1","hook_event_name":"Stop","stop_hook_active":false}"#;
        assert_eq!(
            ClaudeRailCodec
                .decode(RailEvent::BeforeStop, call)
                .unwrap()
                .map(|call| call.act),
            Some(RailAct::BeforeStop)
        );
    }

    #[test]
    fn what_the_engine_says_after_a_write_reaches_the_model_as_context() {
        let call = br#"{"session_id":"s-1","tool_name":"Edit","tool_input":{"file_path":"a.rs"}}"#;
        assert_eq!(
            ClaudeRailCodec
                .decode(RailEvent::AfterWrite, call)
                .unwrap()
                .map(|call| call.act),
            Some(RailAct::AfterWrite {
                paths: vec![PathBuf::from("a.rs")]
            })
        );
        let reply = ClaudeRailCodec.encode(
            RailEvent::AfterWrite,
            &RailAnswer::Tell {
                context: "formatted a.rs".to_string(),
            },
        );
        let said: Value = serde_json::from_slice(&reply.stdout).unwrap();
        assert_eq!(reply.exit, 0);
        assert_eq!(said["hookSpecificOutput"]["hookEventName"], "PostToolUse");
        assert_eq!(
            said["hookSpecificOutput"]["additionalContext"],
            "formatted a.rs"
        );
    }

    #[test]
    fn the_hook_after_a_write_is_installed_only_when_the_engine_formats() {
        let command = RailCommand::new(PathBuf::from("/bin/yunta"));
        let judging = std::time::Duration::from_secs(60);
        let hooks = |formats| {
            let settings: Value =
                serde_json::from_str(&settings_json(&command, None, judging, formats)).unwrap();
            settings["hooks"].clone()
        };
        assert!(hooks(false).get("PostToolUse").is_none());
        let after = &hooks(true)["PostToolUse"][0];
        assert_eq!(after["matcher"], WRITING_TOOLS);
        assert!(after["hooks"][0]["command"]
            .as_str()
            .is_some_and(|command| command.ends_with("rail claude-code after-write")));
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
