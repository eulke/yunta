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
use yunta_core::rail::answer::{FormatAnswer, StopAnswer, WriteAnswer};
use yunta_core::rail::window::{hook_seconds, write_hook_cli_seconds};
use yunta_core::rail::{RailAct, RailCall, RailCommand, RailEvent};
use yunta_core::{ModelName, SessionId};

/// The tools this adapter's write hook matches: every one that writes a
/// file.
const WRITING_TOOLS: &str = "Edit|Write|MultiEdit|NotebookEdit";

pub(crate) struct ClaudeRailCodec;

impl RailCodec for ClaudeRailCodec {
    fn decode(&self, event: RailEvent, stdin: &[u8]) -> Result<RailCall, CodecError> {
        let call: Value = serde_json::from_slice(stdin)?;
        let session_id = call
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<SessionId>().ok())
            .ok_or(CodecError::MissingField("session_id"))?;
        // A model the CLI names is read onto the call; a name that is not a
        // valid model name is read as no name.
        let model = call
            .get("model")
            .and_then(Value::as_str)
            .and_then(|model| model.parse::<ModelName>().ok());
        let act = match event {
            RailEvent::BeforeStop => RailAct::BeforeStop,
            RailEvent::BeforeWrite | RailEvent::AfterWrite => {
                let input = call.get("tool_input").unwrap_or(&Value::Null);
                // `NotebookEdit` names its target `notebook_path`; every
                // other writing tool names it `file_path`.
                let path = ["file_path", "notebook_path"]
                    .iter()
                    .find_map(|field| input.get(*field).and_then(Value::as_str))
                    .ok_or(CodecError::MissingField("tool_input.file_path"))?;
                let paths = vec![PathBuf::from(path)];
                match event {
                    RailEvent::AfterWrite => RailAct::AfterWrite { paths },
                    _ => RailAct::BeforeWrite { paths },
                }
            }
        };
        Ok(RailCall {
            session_id,
            act,
            model,
        })
    }

    fn before_write(&self, answer: &WriteAnswer) -> HookReply {
        crate::session::before_write_reply(answer)
    }

    fn after_write(&self, answer: &FormatAnswer) -> HookReply {
        crate::session::after_write_reply(answer)
    }

    fn before_stop(&self, answer: &StopAnswer) -> HookReply {
        crate::session::before_stop_reply(answer)
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
                    "timeout": write_hook_cli_seconds(),
                }],
            }]),
        ),
        (
            "Stop".to_string(),
            serde_json::json!([{
                "hooks": [{
                    "type": "command",
                    "command": hook(RailEvent::BeforeStop),
                    "timeout": hook_seconds(judging),
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
                    "timeout": write_hook_cli_seconds(),
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
    use yunta_core::rail::answer::{FormatNote, WriteRefusal, REFUSAL_MARKER};
    use yunta_core::CommandName;

    use super::*;

    fn decoded(call: &[u8]) -> Result<RailCall, CodecError> {
        ClaudeRailCodec.decode(RailEvent::BeforeWrite, call)
    }

    #[test]
    fn a_notebook_edit_names_its_target_by_its_own_field() {
        let call = br#"{"session_id":"s-1","tool_name":"NotebookEdit","tool_input":{"notebook_path":"a.ipynb"}}"#;
        assert_eq!(
            decoded(call).unwrap().act,
            RailAct::BeforeWrite {
                paths: vec![PathBuf::from("a.ipynb")]
            }
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
    fn a_model_the_cli_names_is_read_onto_the_call() {
        let named = br#"{"session_id":"s-1","model":"claude-sonnet-4","tool_name":"Edit","tool_input":{"file_path":"a.rs"}}"#;
        let unnamed =
            br#"{"session_id":"s-1","tool_name":"Edit","tool_input":{"file_path":"a.rs"}}"#;
        assert_eq!(
            decoded(named).unwrap().model.map(|model| model.to_string()),
            Some("claude-sonnet-4".to_string())
        );
        assert!(decoded(unnamed).unwrap().model.is_none());
    }

    #[test]
    fn a_refused_stop_keeps_the_session_working_with_the_reason() {
        let reply = ClaudeRailCodec.before_stop(&StopAnswer::Refuse {
            reason: "a criterion is red".to_string(),
        });
        let said: Value = serde_json::from_slice(&reply.stdout).unwrap();
        assert_eq!(reply.exit, 0);
        assert_eq!(said["decision"], "block");
        assert_eq!(said["reason"], "a criterion is red");
        let call = br#"{"session_id":"s-1","hook_event_name":"Stop","stop_hook_active":false}"#;
        assert_eq!(
            ClaudeRailCodec
                .decode(RailEvent::BeforeStop, call)
                .unwrap()
                .act,
            RailAct::BeforeStop
        );
    }

    #[test]
    fn what_the_engine_says_after_a_write_reaches_the_model_as_context() {
        let call = br#"{"session_id":"s-1","tool_name":"Edit","tool_input":{"file_path":"a.rs"}}"#;
        assert_eq!(
            ClaudeRailCodec
                .decode(RailEvent::AfterWrite, call)
                .unwrap()
                .act,
            RailAct::AfterWrite {
                paths: vec![PathBuf::from("a.rs")]
            }
        );
        let reply = ClaudeRailCodec.after_write(&FormatAnswer::Tell {
            notes: vec![FormatNote::Changed {
                run: "fmt".parse::<CommandName>().unwrap(),
                paths: vec![PathBuf::from("a.rs")],
            }],
        });
        let said: Value = serde_json::from_slice(&reply.stdout).unwrap();
        assert_eq!(reply.exit, 0);
        assert_eq!(said["hookSpecificOutput"]["hookEventName"], "PostToolUse");
        assert_eq!(
            said["hookSpecificOutput"]["additionalContext"],
            "yunta: `fmt` formatted `a.rs` after your write; read it again before you edit it."
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
        let refusal = WriteRefusal::try_from(format!("{REFUSAL_MARKER}no")).unwrap();
        let reply = ClaudeRailCodec.before_write(&WriteAnswer::Refuse { reason: refusal });
        assert_eq!(
            (reply.exit, reply.stderr),
            (2, format!("{REFUSAL_MARKER}no").into_bytes())
        );
    }

    #[test]
    fn a_sub_second_stop_window_still_gives_the_hook_one_second() {
        let command = RailCommand::new(PathBuf::from("/bin/yunta"));
        let settings: Value = serde_json::from_str(&settings_json(
            &command,
            None,
            std::time::Duration::from_millis(300),
            false,
        ))
        .unwrap();
        assert_eq!(settings["hooks"]["Stop"][0]["hooks"][0]["timeout"], 1);
    }
}
