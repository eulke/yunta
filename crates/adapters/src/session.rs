//! What every adapter in this crate shares: reading its own settings,
//! handing a prompt to a CLI that already runs, and naming what a tool
//! acted on without repeating the thing itself.
//!
//! The port these adapters implement is `yunta_core::port`.

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

/// How long a CLI waits for its write hook to answer. A hook that does not
/// answer in time lets the call through — so the hook itself gives up on
/// the engine sooner and refuses, and the post-check diff catches anything
/// else.
pub(crate) const HOOK_TIMEOUT_SECONDS: u32 = 10;

/// What a hook after a write answers to tell the model something about
/// it: the context both CLIs add to what the model reads next.
pub(crate) fn told_after_write(context: &str) -> yunta_core::port::HookReply {
    let said = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": context,
        }
    });
    yunta_core::port::HookReply {
        stdout: said.to_string().into_bytes(),
        stderr: Vec::new(),
        exit: 0,
    }
}

/// What a stop hook answers to keep a session working: the decision both
/// CLIs read on stdout, with the reason the model is shown.
pub(crate) fn kept_working(reason: &str) -> yunta_core::port::HookReply {
    let said = serde_json::json!({ "decision": "block", "reason": reason });
    yunta_core::port::HookReply {
        stdout: said.to_string().into_bytes(),
        stderr: Vec::new(),
        exit: 0,
    }
}

/// The answer a stop hook's reply gives the session: `Refuse` with the
/// reason when stdout blocks the stop, which is how both CLIs keep a session
/// working, and `Proceed` otherwise.
pub(crate) fn stop_answer(reply: &yunta_core::port::HookReply) -> yunta_core::rail::RailAnswer {
    let said: serde_json::Value = serde_json::from_slice(&reply.stdout).unwrap_or_default();
    match said.get("decision").and_then(serde_json::Value::as_str) {
        Some("block") => yunta_core::rail::RailAnswer::Refuse {
            reason: said
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => yunta_core::rail::RailAnswer::Proceed,
    }
}

/// The answer an after-write hook's reply gives the session: `Tell` with the
/// context the model is shown, when stdout carries one, and `Proceed`
/// otherwise.
pub(crate) fn after_write_answer(
    reply: &yunta_core::port::HookReply,
) -> yunta_core::rail::RailAnswer {
    let said: serde_json::Value = serde_json::from_slice(&reply.stdout).unwrap_or_default();
    match said
        .pointer("/hookSpecificOutput/additionalContext")
        .and_then(serde_json::Value::as_str)
    {
        Some(context) => yunta_core::rail::RailAnswer::Tell {
            context: context.to_string(),
        },
        None => yunta_core::rail::RailAnswer::Proceed,
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
