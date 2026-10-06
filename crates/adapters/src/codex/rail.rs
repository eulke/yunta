//! How the `codex` CLI's hooks ask the engine, and what they read back.
//!
//! Codex runs a `PreToolUse` hook before every `apply_patch` and hands it
//! the call as JSON on stdin, the patch in `tool_input.command`. One patch
//! can add, change, delete and move several files, so every path it names
//! is asked about at once. Exit 2 with the reason on stderr refuses the
//! patch, and the model reads the reason; exit 0 lets it through.
//!
//! The hook reaches the CLI as a `-c` override, and Codex runs a hook from
//! an override only once it is trusted. The trust is the hook's own hash,
//! recorded under its key in `hooks.state` — so this adapter hands over the
//! hash of exactly the hook it installs, in the same call, rather than
//! letting every untrusted hook run.

use std::path::PathBuf;

use serde_json::Value;
use toml::{Table, Value as Toml};
use yunta_core::port::{CodecError, HookReply, RailCodec};
use yunta_core::rail::{RailAct, RailAnswer, RailCall, RailCommand, RailEvent};
use yunta_core::SessionId;

use super::config::ConfigOverride;

/// The tool this adapter's write hook matches: every file edit Codex
/// makes is an `apply_patch`.
const WRITING_TOOL: &str = "apply_patch";

/// Where the CLI keeps the hooks an invocation's `-c` overrides declare,
/// as their trust keys name it.
const SESSION_FLAGS: &str = "/<session-flags>/config.toml";

pub(super) struct CodexRailCodec;

impl RailCodec for CodexRailCodec {
    fn decode(&self, event: RailEvent, stdin: &[u8]) -> Result<Option<RailCall>, CodecError> {
        let call: Value = serde_json::from_slice(stdin)?;
        let session_id = call
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse::<SessionId>().ok())
            .ok_or(CodecError::MissingField("session_id"))?;
        match event {
            RailEvent::BeforeWrite => {
                let patch = call
                    .get("tool_input")
                    .and_then(|input| input.get("command"))
                    .and_then(Value::as_str)
                    .ok_or(CodecError::MissingField("tool_input.command"))?;
                let paths = patch_paths(patch);
                if paths.is_empty() {
                    return Err(CodecError::MissingField("a file the patch names"));
                }
                Ok(Some(RailCall {
                    session_id,
                    act: RailAct::BeforeWrite { paths },
                }))
            }
        }
    }

    fn encode(&self, event: RailEvent, answer: &RailAnswer) -> HookReply {
        match (event, answer) {
            // Codex reads a refusal with nothing on stderr as a hook that
            // failed, and runs the patch anyway: the reason is never empty.
            (RailEvent::BeforeWrite, RailAnswer::Refuse { reason }) => HookReply {
                stdout: Vec::new(),
                stderr: match reason.is_empty() {
                    true => b"yunta: write refused".to_vec(),
                    false => reason.clone().into_bytes(),
                },
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

/// Every path one patch writes, moves to or deletes, in the order it
/// names them, as written — relative to the session's working directory
/// or absolute.
pub(super) fn patch_paths(patch: &str) -> Vec<PathBuf> {
    const HEADERS: [&str; 4] = [
        "*** Add File: ",
        "*** Update File: ",
        "*** Delete File: ",
        "*** Move to: ",
    ];
    patch
        .lines()
        .filter_map(|line| {
            HEADERS
                .iter()
                .find_map(|header| line.strip_prefix(header))
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .collect()
}

/// The `-c` overrides that install the hooks and trust them.
pub(super) fn hook_overrides(command: &RailCommand) -> Vec<String> {
    let argv = command
        .argv(&super::ID, RailEvent::BeforeWrite)
        .iter()
        .map(|part| crate::session::sh_quoted(part))
        .collect::<Vec<_>>()
        .join(" ");
    let handler = Table::from_iter([
        ("type".to_string(), Toml::String("command".to_string())),
        ("command".to_string(), Toml::String(argv.clone())),
        (
            "timeout".to_string(),
            Toml::Integer(i64::from(crate::session::HOOK_TIMEOUT_SECONDS)),
        ),
    ]);
    let group = Table::from_iter([
        (
            "matcher".to_string(),
            Toml::String(WRITING_TOOL.to_string()),
        ),
        ("hooks".to_string(), Toml::Array(vec![Toml::Table(handler)])),
    ]);
    let key = format!("{SESSION_FLAGS}:pre_tool_use:0:0");
    let trust = Table::from_iter([(
        key,
        Toml::Table(Table::from_iter([(
            "trusted_hash".to_string(),
            Toml::String(trusted_hash("pre_tool_use", Some(WRITING_TOOL), &argv)),
        )])),
    )]);
    [
        ConfigOverride::value("hooks.PreToolUse", Toml::Array(vec![Toml::Table(group)])),
        ConfigOverride::value("hooks.state", Toml::Table(trust)),
    ]
    .into_iter()
    .flat_map(ConfigOverride::into_args)
    .collect()
}

/// The hash Codex trusts one command hook by: SHA-256 over the hook's
/// normalized identity — its event, its matcher and its one handler with
/// every default written out — as compact JSON with its keys sorted.
fn trusted_hash(event: &str, matcher: Option<&str>, command: &str) -> String {
    let mut identity = serde_json::Map::new();
    identity.insert("event_name".to_string(), Value::from(event));
    if let Some(matcher) = matcher {
        identity.insert("matcher".to_string(), Value::from(matcher));
    }
    identity.insert(
        "hooks".to_string(),
        serde_json::json!([{
            "async": false,
            "command": command,
            "timeout": crate::session::HOOK_TIMEOUT_SECONDS,
            "type": "command",
        }]),
    );
    let canonical = sorted(&Value::Object(identity)).to_string();
    yunta_core::sha256_hex(canonical.as_bytes()).qualified()
}

/// `value` with every object's keys in order, whatever order the map kept.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: std::collections::BTreeMap<&String, Value> = map
                .iter()
                .map(|(key, value)| (key, sorted(value)))
                .collect();
            Value::Object(
                ordered
                    .into_iter()
                    .map(|(key, value)| (key.clone(), value))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(patch: &str) -> Vec<u8> {
        serde_json::json!({
            "session_id": "s-1",
            "tool_name": "apply_patch",
            "tool_input": { "command": patch },
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn a_patch_names_every_file_it_adds_changes_deletes_or_moves_to() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+hello\n*** Update File: src/b.rs\n*** Move to: src/c.rs\n@@\n-x\n+y\n*** Delete File: /work/old.rs\n*** End Patch";
        assert_eq!(
            patch_paths(patch),
            ["a.txt", "src/b.rs", "src/c.rs", "/work/old.rs"].map(PathBuf::from)
        );
    }

    #[test]
    fn a_call_reads_into_one_question_about_every_path() {
        let decoded = CodexRailCodec
            .decode(
                RailEvent::BeforeWrite,
                &call("*** Begin Patch\n*** Add File: a.txt\n+x\n*** Add File: b/c.txt\n+y\n*** End Patch"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(decoded.session_id.as_str(), "s-1");
        assert_eq!(
            decoded.act,
            RailAct::BeforeWrite {
                paths: vec![PathBuf::from("a.txt"), PathBuf::from("b/c.txt")]
            }
        );
    }

    #[test]
    fn a_patch_that_names_no_file_is_a_call_this_codec_cannot_read() {
        assert!(CodexRailCodec
            .decode(
                RailEvent::BeforeWrite,
                &call("*** Begin Patch\n*** End Patch")
            )
            .is_err());
    }

    #[test]
    fn a_refusal_never_leaves_stderr_empty() {
        let reply = CodexRailCodec.encode(
            RailEvent::BeforeWrite,
            &RailAnswer::Refuse {
                reason: String::new(),
            },
        );
        assert_eq!(reply.exit, 2);
        assert!(!reply.stderr.is_empty());
    }

    /// The hash of a hook the person's own Codex trusts, as its
    /// configuration recorded it.
    #[test]
    fn the_trusted_hash_is_the_one_codex_records() {
        assert_eq!(
            trusted_hash("session_start", None, "bash '/hooks/state.sh' session")
                .strip_prefix("sha256:")
                .map(str::len),
            Some(64)
        );
        let command = "python3 /tmp/rail.py pre";
        assert_eq!(
            trusted_hash("pre_tool_use", Some("apply_patch"), command),
            yunta_core::sha256_hex(
                br#"{"event_name":"pre_tool_use","hooks":[{"async":false,"command":"python3 /tmp/rail.py pre","timeout":10,"type":"command"}],"matcher":"apply_patch"}"#
            )
            .qualified()
        );
    }

    #[test]
    fn the_overrides_read_back_as_the_hook_and_its_trust() {
        let args = hook_overrides(&RailCommand::new(PathBuf::from("/bin/yunta")));
        let assignments: Vec<&String> = args.iter().skip(1).step_by(2).collect();
        let hooks: toml::Table = toml::from_str(assignments[0]).unwrap();
        let group = &hooks["hooks"]["PreToolUse"][0];
        assert_eq!(group["matcher"].as_str(), Some("apply_patch"));
        assert_eq!(
            group["hooks"][0]["command"].as_str(),
            Some("/bin/yunta rail codex before-write")
        );
        let state: toml::Table = toml::from_str(assignments[1]).unwrap();
        let trusted = &state["hooks"]["state"]["/<session-flags>/config.toml:pre_tool_use:0:0"];
        assert!(trusted["trusted_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:")));
    }
}
