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

/// The confinement a session's commands run under, as both CLIs name
/// their modes: nothing written but the run's roots, the workspace and the
/// roots, or anything. `adapter_settings.sandbox` picks the mode an `edit`
/// session runs under; `read_only` and `full` map to their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Sandbox {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    DangerFullAccess,
}

impl Sandbox {
    /// The mode a session under `profile` runs in, `edit` taking the
    /// configured one.
    pub(crate) fn of(profile: yunta_core::port::PermissionProfile, edit: Sandbox) -> Sandbox {
        match profile {
            yunta_core::port::PermissionProfile::ReadOnly => Sandbox::ReadOnly,
            yunta_core::port::PermissionProfile::Edit => edit,
            yunta_core::port::PermissionProfile::Full => Sandbox::DangerFullAccess,
        }
    }

    /// The mode as a command line spells it.
    pub(crate) fn as_flag(self) -> &'static str {
        match self {
            Sandbox::ReadOnly => "read-only",
            Sandbox::WorkspaceWrite => "workspace-write",
            Sandbox::DangerFullAccess => "danger-full-access",
        }
    }
}

/// How long a CLI waits for its write hook to answer. A hook that does not
/// answer in time lets the call through — so the hook itself gives up on
/// the engine sooner and refuses, and the post-check diff catches anything
/// else.
pub(crate) const HOOK_TIMEOUT_SECONDS: u32 = 10;

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
