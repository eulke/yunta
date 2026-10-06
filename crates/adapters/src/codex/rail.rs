//! How the `codex` CLI's hooks ask the engine, and what they read back.
//!
//! Codex runs a `PreToolUse` hook before every `apply_patch` and hands it
//! the call as JSON on stdin, the patch in `tool_input.command`. One patch
//! can add, change, delete and move several files, so every path it names
//! is asked about at once. Exit 2 with the reason on stderr refuses the
//! patch, and the model reads the reason; exit 0 lets it through. A
//! `PostToolUse` hook after the same patch hands the model
//! `additionalContext` when the engine has something to say about what it
//! wrote. Its
//! `Stop` hook runs when the session would end its turn, and
//! `{"decision":"block"}` with a reason on stdout keeps it working.
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
            RailEvent::BeforeStop => Ok(Some(RailCall {
                session_id,
                act: RailAct::BeforeStop,
            })),
            RailEvent::BeforeWrite | RailEvent::AfterWrite => {
                let patch = call
                    .get("tool_input")
                    .and_then(|input| input.get("command"))
                    .and_then(Value::as_str)
                    .ok_or(CodecError::MissingField("tool_input.command"))?;
                let paths = patch_paths(patch);
                if paths.is_empty() {
                    return Err(CodecError::MissingField("a file the patch names"));
                }
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

/// One hook this adapter installs: the CLI's event, as its config and its
/// trust keys name it, the tool it matches, and how long the CLI waits.
struct Hook {
    event: &'static str,
    key: &'static str,
    matcher: Option<&'static str>,
    rail: RailEvent,
    timeout: u64,
}

/// The `-c` overrides that install the hooks and trust them. The stop
/// hook waits for the engine as long as `judging` allows: judging the
/// work runs its checks. The hook after a patch is installed only when the
/// engine `formats` what it wrote.
pub(super) fn hook_overrides(
    command: &RailCommand,
    judging: std::time::Duration,
    formats: bool,
) -> Vec<String> {
    let mut trust = Table::new();
    let mut overrides = Vec::new();
    for hook in installed(judging, formats) {
        let argv = command
            .argv(&super::ID, hook.rail)
            .iter()
            .map(|part| crate::session::sh_quoted(part))
            .collect::<Vec<_>>()
            .join(" ");
        overrides.push(ConfigOverride::value(
            format!("hooks.{}", hook.event),
            Toml::Array(vec![Toml::Table(group(&hook, &argv))]),
        ));
        let hash = trusted_hash(hook.key, hook.matcher, &argv, hook.timeout);
        trust.insert(
            format!("{SESSION_FLAGS}:{}:0:0", hook.key),
            Toml::Table(Table::from_iter([(
                "trusted_hash".to_string(),
                Toml::String(hash),
            )])),
        );
    }
    overrides.push(ConfigOverride::value("hooks.state", Toml::Table(trust)));
    overrides
        .into_iter()
        .flat_map(ConfigOverride::into_args)
        .collect()
}

/// The hooks a session gets: before a patch and before a stop always,
/// after a patch when the engine formats what it wrote.
fn installed(judging: std::time::Duration, formats: bool) -> Vec<Hook> {
    let patching = |event, key, rail| Hook {
        event,
        key,
        matcher: Some(WRITING_TOOL),
        rail,
        timeout: u64::from(crate::session::HOOK_TIMEOUT_SECONDS),
    };
    let mut hooks = vec![
        patching("PreToolUse", "pre_tool_use", RailEvent::BeforeWrite),
        Hook {
            event: "Stop",
            key: "stop",
            matcher: None,
            rail: RailEvent::BeforeStop,
            timeout: judging.as_secs(),
        },
    ];
    if formats {
        hooks.push(patching(
            "PostToolUse",
            "post_tool_use",
            RailEvent::AfterWrite,
        ));
    }
    hooks
}

/// `hook`'s one group as the CLI's config reads it: its matcher, and its
/// one handler running `argv`.
fn group(hook: &Hook, argv: &str) -> Table {
    let handler = Table::from_iter([
        ("type".to_string(), Toml::String("command".to_string())),
        ("command".to_string(), Toml::String(argv.to_string())),
        (
            "timeout".to_string(),
            Toml::Integer(i64::try_from(hook.timeout).unwrap_or(i64::MAX)),
        ),
    ]);
    let mut group = Table::new();
    if let Some(matcher) = hook.matcher {
        group.insert("matcher".to_string(), Toml::String(matcher.to_string()));
    }
    group.insert("hooks".to_string(), Toml::Array(vec![Toml::Table(handler)]));
    group
}

/// The hash Codex trusts one command hook by: SHA-256 over the hook's
/// normalized identity — its event, its matcher and its one handler with
/// every default written out — as compact JSON with its keys sorted.
fn trusted_hash(event: &str, matcher: Option<&str>, command: &str, timeout: u64) -> String {
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
            "timeout": timeout,
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
            trusted_hash("session_start", None, "bash '/hooks/state.sh' session", 10)
                .strip_prefix("sha256:")
                .map(str::len),
            Some(64)
        );
        let command = "python3 /tmp/rail.py pre";
        assert_eq!(
            trusted_hash("pre_tool_use", Some("apply_patch"), command, 10),
            yunta_core::sha256_hex(
                br#"{"event_name":"pre_tool_use","hooks":[{"async":false,"command":"python3 /tmp/rail.py pre","timeout":10,"type":"command"}],"matcher":"apply_patch"}"#
            )
            .qualified()
        );
    }

    #[test]
    fn the_overrides_read_back_as_the_hooks_and_their_trust() {
        let judging = std::time::Duration::from_secs(900);
        let args = hook_overrides(
            &RailCommand::new(PathBuf::from("/bin/yunta")),
            judging,
            false,
        );
        let assignments: Vec<&String> = args.iter().skip(1).step_by(2).collect();
        let hooks: toml::Table = toml::from_str(assignments[0]).unwrap();
        let group = &hooks["hooks"]["PreToolUse"][0];
        assert_eq!(group["matcher"].as_str(), Some("apply_patch"));
        assert_eq!(
            group["hooks"][0]["command"].as_str(),
            Some("/bin/yunta rail codex before-write")
        );
        let stops: toml::Table = toml::from_str(assignments[1]).unwrap();
        let group = &stops["hooks"]["Stop"][0];
        assert!(group.get("matcher").is_none());
        assert_eq!(
            group["hooks"][0]["command"].as_str(),
            Some("/bin/yunta rail codex before-stop")
        );
        assert_eq!(group["hooks"][0]["timeout"].as_integer(), Some(900));
        let state: toml::Table = toml::from_str(assignments[2]).unwrap();
        let trusted = |key: &str| {
            state["hooks"]["state"][format!("/<session-flags>/config.toml:{key}:0:0").as_str()]
                ["trusted_hash"]
                .as_str()
                .map(str::to_string)
        };
        assert_eq!(
            trusted("stop"),
            Some(trusted_hash(
                "stop",
                None,
                "/bin/yunta rail codex before-stop",
                900
            ))
        );
        assert!(trusted("pre_tool_use").is_some_and(|hash| hash.starts_with("sha256:")));
    }

    #[test]
    fn the_hook_after_a_patch_is_installed_and_trusted_only_when_the_engine_formats() {
        let command = RailCommand::new(PathBuf::from("/bin/yunta"));
        let judging = std::time::Duration::from_secs(60);
        let formatting = hook_overrides(&command, judging, true);
        let quiet = hook_overrides(&command, judging, false);
        assert!(!quiet
            .iter()
            .any(|arg| arg.starts_with("hooks.PostToolUse=")));
        let after = formatting
            .iter()
            .find(|arg| arg.starts_with("hooks.PostToolUse="))
            .expect("the hook after a patch is installed");
        let after: toml::Table = toml::from_str(after).unwrap();
        let group = &after["hooks"]["PostToolUse"][0];
        assert_eq!(group["matcher"].as_str(), Some("apply_patch"));
        assert_eq!(
            group["hooks"][0]["command"].as_str(),
            Some("/bin/yunta rail codex after-write")
        );
        let state = formatting
            .iter()
            .find(|arg| arg.starts_with("hooks.state="))
            .unwrap();
        assert!(state.contains("/<session-flags>/config.toml:post_tool_use:0:0"));
    }

    #[test]
    fn a_refused_stop_keeps_the_session_working_with_the_reason() {
        let reply = CodexRailCodec.encode(
            RailEvent::BeforeStop,
            &RailAnswer::Refuse {
                reason: "a criterion is red".to_string(),
            },
        );
        let said: Value = serde_json::from_slice(&reply.stdout).unwrap();
        assert_eq!((reply.exit, said["decision"].as_str()), (0, Some("block")));
        assert_eq!(said["reason"], "a criterion is red");
    }
}
