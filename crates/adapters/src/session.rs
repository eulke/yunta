//! What every adapter in this crate shares: reading its own settings,
//! handing a prompt to a CLI that already runs, naming what a tool
//! acted on without repeating the thing itself, and the replies a hook
//! leaves for the CLI that runs it.
//!
//! The port these adapters implement is `yunta_core::port`.

use yunta_core::port::HookReply;
use yunta_core::rail::answer::{FormatAnswer, StopAnswer, WriteAnswer};
use yunta_core::{AdapterError, AdapterId, Result};

/// Reads an adapter's `adapter_settings` map into its typed settings:
/// every key must be one of `known`, and the typed struct reads the
/// values. The one place a setting name is checked, so an unknown key
/// is always the same error whichever adapter it reaches.
pub fn typed_settings<T: serde::de::DeserializeOwned>(
    adapter: &'static AdapterId,
    raw: Option<&serde_json::Map<String, serde_json::Value>>,
    known: &'static [&'static str],
) -> Result<T> {
    let raw = raw.cloned().unwrap_or_default();
    if let Some(key) = raw.keys().find(|key| !known.contains(&key.as_str())) {
        return Err(AdapterError::UnknownSetting {
            adapter: adapter.clone(),
            key: key.clone(),
            known: known.to_vec(),
        });
    }
    serde_json::from_value(serde_json::Value::Object(raw)).map_err(|e| AdapterError::Adapter {
        adapter: adapter.clone(),
        message: format!("`adapter_settings`: {e}"),
    })
}

/// The reply that lets a hook's CLI go on: nothing printed on either
/// stream, and exit 0.
fn goes_on() -> HookReply {
    HookReply {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit: 0,
    }
}

/// What a hook after a write answers to tell the model something about
/// it: the context both CLIs add to what the model reads next.
pub(crate) fn told_after_write(context: &str) -> HookReply {
    let said = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": context,
        }
    });
    HookReply {
        stdout: said.to_string().into_bytes(),
        stderr: Vec::new(),
        exit: 0,
    }
}

/// What a stop hook answers to keep a session working: the decision both
/// CLIs read on stdout, with the reason the model is shown.
pub(crate) fn kept_working(reason: &str) -> HookReply {
    let said = serde_json::json!({ "decision": "block", "reason": reason });
    HookReply {
        stdout: said.to_string().into_bytes(),
        stderr: Vec::new(),
        exit: 0,
    }
}

/// What a write hook answers to the engine's verdict on a write: nothing
/// when the write may go on, and the refusal's text on stderr with exit 2
/// when it may not, which both CLIs read as a denial.
pub(crate) fn before_write_reply(answer: &WriteAnswer) -> HookReply {
    match answer {
        WriteAnswer::Proceed => goes_on(),
        WriteAnswer::Refuse { reason } => HookReply {
            stdout: Vec::new(),
            stderr: String::from(reason.clone()).into_bytes(),
            exit: 2,
        },
    }
}

/// What an after-write hook answers to the notes the engine made of a
/// write: nothing when it has none, and otherwise each note's text, told
/// to the model as one context.
pub(crate) fn after_write_reply(answer: &FormatAnswer) -> HookReply {
    match answer {
        FormatAnswer::Proceed => goes_on(),
        FormatAnswer::Tell { notes } => {
            let texts: Vec<String> = notes.iter().map(ToString::to_string).collect();
            told_after_write(&texts.join("\n\n"))
        }
    }
}

/// What a stop hook answers to the engine's judgement of a stop: nothing
/// when the stop may go on, and the reason that keeps the session working
/// when it may not.
pub(crate) fn before_stop_reply(answer: &StopAnswer) -> HookReply {
    match answer {
        StopAnswer::Proceed => goes_on(),
        StopAnswer::Refuse { reason } => kept_working(reason),
    }
}

/// `part` as one word of a POSIX shell command line: as written when it
/// holds nothing the shell would read, and in single quotes otherwise,
/// where nothing is expanded — a binary under a path with a `$`, a space
/// or a quote runs as itself. A CLI runs a hook's command through a
/// shell, so every word of one goes through here.
pub(crate) fn sh_quoted(part: &str) -> String {
    let plain = !part.is_empty()
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./=:-@%+,".contains(c));
    match plain {
        true => part.to_string(),
        false => format!("'{}'", part.replace('\'', r"'\''")),
    }
}

#[cfg(test)]
mod tests {
    use super::sh_quoted;

    #[test]
    fn a_hook_command_survives_a_path_the_shell_would_expand() {
        let path = "/Users/a b/$HOME/it's/yunta";
        let quoted = sh_quoted(path);
        let echoed = std::process::Command::new("sh")
            .args(["-c", &format!("printf %s {quoted}")])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(echoed.stdout).unwrap(), path);
        assert_eq!(sh_quoted("/usr/local/bin/yunta"), "/usr/local/bin/yunta");
    }
}
