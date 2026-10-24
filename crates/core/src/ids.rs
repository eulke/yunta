//! Newtyped identifiers, parsed once at the frontier.
//!
//! Every identifier checks its rule when it is built from text — through
//! `FromStr`, `TryFrom` or `Deserialize` — so an invalid value is
//! unrepresentable past the parser that read it, and two kinds of id can
//! never be swapped by accident. The rules are the ones the schema and
//! the tasks document; each type's rustdoc states its own.
//!
//! Unchecked construction from a literal (`From<&str>`) exists only for
//! tests, behind the `testkit` feature; production code parses through
//! `FromStr` or `TryFrom<String>`.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

mod composite;
mod finding;
mod node;
mod process;
mod run;
mod sequence;
mod task;
#[cfg(test)]
mod tests;

pub use composite::{GitHubRepo, PackRef};
pub use finding::FindingId;
pub use node::NodeId;
pub use process::Pid;
pub use run::RunId;
pub use sequence::Seq;
pub use task::TaskId;

/// A value that breaks the rule of the identifier it was meant to be.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("`{value}` is not a valid {what}: {rule}")]
pub struct InvalidId {
    /// What the value was meant to be: `node id`, `runner name`, ...
    pub what: &'static str,
    pub value: String,
    /// The rule the value breaks, as a reader can act on it.
    pub rule: &'static str,
}

// --- Rules -------------------------------------------------------------
//
// Each rule is a `const fn` over bytes so a literal can be checked while
// the program is built (`from_static`), and the same function checks
// every value parsed at runtime.

/// The rule every short name an author or an agent writes follows:
/// `^[A-Za-z][A-Za-z0-9_-]*$`.
const NAME_RULE: &str = "a letter followed by letters, digits, `_` or `-`";

const fn is_name(bytes: &[u8]) -> bool {
    let Some((first, mut rest)) = bytes.split_first() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    while let Some((byte, tail)) = rest.split_first() {
        if !(byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-') {
            return false;
        }
        rest = tail;
    }
    true
}

const SHARED_DIR_VAR_RULE: &str =
    "an uppercase letter or `_` followed by uppercase letters, digits or `_`, and not `PATH`";

/// A variable a shared directory is exported under: shaped like an
/// environment variable, and never `PATH`, which lists directories
/// rather than naming one.
const fn is_shared_dir_var(bytes: &[u8]) -> bool {
    if let [b'P', b'A', b'T', b'H'] = bytes {
        return false;
    }
    let Some((first, mut rest)) = bytes.split_first() else {
        return false;
    };
    if !(first.is_ascii_uppercase() || *first == b'_') {
        return false;
    }
    while let Some((byte, tail)) = rest.split_first() {
        if !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_') {
            return false;
        }
        rest = tail;
    }
    true
}

/// A node id is a name, or a name plus `@` and the runner's name for a
/// fan-out sibling the manifest expands `runners:` into.
const NODE_RULE: &str =
    "a letter followed by letters, digits, `_` or `-`; a fan-out sibling adds `@` and its runner's name";

const FAN_OUT_SEPARATOR: u8 = b'@';

const fn is_node_id(bytes: &[u8]) -> bool {
    let mut i = 0;
    let mut rest = bytes;
    while let Some((byte, tail)) = rest.split_first() {
        if *byte == FAN_OUT_SEPARATOR {
            let (base, after) = bytes.split_at(i);
            let (_, runner) = after.split_at(1);
            return is_name(base) && is_name(runner);
        }
        rest = tail;
        i += 1;
    }
    is_name(bytes)
}

/// Names an adapter defines — a model, an agent — are opaque to the
/// engine beyond being one printable word.
const TOKEN_RULE: &str = "printable ASCII without whitespace";

const fn is_token(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut rest = bytes;
    while let Some((byte, tail)) = rest.split_first() {
        if !byte.is_ascii_graphic() {
            return false;
        }
        rest = tail;
    }
    true
}

/// What a value must be to serve as one directory name: under
/// `.yunta/packs/<publisher>/<name>/`, under `runs/<run id>/`.
const SEGMENT_RULE: &str =
    "one path segment: printable ASCII without whitespace, `/` or `\\`, and not `.` or `..`";

const fn is_segment(bytes: &[u8]) -> bool {
    if !is_token(bytes) {
        return false;
    }
    if matches!(bytes, b"." | b"..") {
        return false;
    }
    let mut rest = bytes;
    while let Some((byte, tail)) = rest.split_first() {
        if *byte == b'/' || *byte == b'\\' {
            return false;
        }
        rest = tail;
    }
    true
}

/// A session id is whatever the adapter's CLI issued: kept opaque,
/// refused only when it could not name a session at all.
const OPAQUE_RULE: &str = "not empty and without control characters";

const fn is_opaque(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut rest = bytes;
    while let Some((byte, tail)) = rest.split_first() {
        if *byte < 0x20 || *byte == 0x7F {
            return false;
        }
        rest = tail;
    }
    true
}

/// Whether `value` is one path segment — the rule a run id, a publisher
/// and a pack name follow, exposed for values that are path segments
/// without being identifiers of their own (a workflow file's basename).
pub fn is_path_segment(value: &str) -> bool {
    is_segment(value.as_bytes())
}

// --- String identifiers ------------------------------------------------

/// The indefinite article a schema description opens with, by the
/// noun's first letter; the descriptions are generated, so the grammar
/// is decided here once.
pub(crate) fn article(noun: &str) -> &'static str {
    if noun.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "An"
    } else {
        "A"
    }
}

/// Defines a newtyped string identifier: its rule, its parsers, its
/// serde forms, and — under `testkit` — unchecked `From` for tests.
macro_rules! string_id {
    (
        $(#[$doc:meta])*
        $name:ident, what = $what:literal, rule = $rule:ident, check = $check:ident
    ) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Cow<'static, str>);

        impl $name {
            /// Builds the identifier from a literal, checked while the
            /// program is built: in a `const` or `static` initializer a
            /// value that breaks the rule is a compile error.
            pub const fn from_static(value: &'static str) -> Self {
                if !$check(value.as_bytes()) {
                    panic!(concat!("`from_static` was given a value that is not a valid ", $what));
                }
                Self(Cow::Borrowed(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            fn checked(value: String) -> Result<Self, InvalidId> {
                if $check(value.as_bytes()) {
                    Ok(Self(Cow::Owned(value)))
                } else {
                    Err(InvalidId {
                        what: $what,
                        value,
                        rule: $rule,
                    })
                }
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.as_str()).finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.as_str() == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(value: &str) -> Result<Self, InvalidId> {
                Self::checked(value.to_string())
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidId;

            fn try_from(value: String) -> Result<Self, InvalidId> {
                Self::checked(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::checked(value).map_err(serde::de::Error::custom)
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({
                    "type": "string",
                    "minLength": 1,
                    "description": format!("{} {}: {}", $crate::ids::article($what), $what, $rule),
                })
            }
        }

        /// Test convenience: a literal that breaks the rule panics with
        /// the rule. Production code parses instead; `TryFrom<&str>`
        /// exists only through this impl, so it is not a parser.
        #[cfg(any(test, feature = "testkit"))]
        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                match value.parse() {
                    Ok(id) => id,
                    Err(error) => panic!("{error}"),
                }
            }
        }
    };
}

pub(crate) use string_id;

string_id!(
    /// A runner's name — the key under `runners:` a node's `runner:`
    /// and a pack's `requires.runners` refer to:
    /// `^[A-Za-z][A-Za-z0-9_-]*$`.
    RunnerName, what = "runner name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// An adapter's id as config names it (`claude-code`, `codex`,
    /// `mock`): `^[A-Za-z][A-Za-z0-9_-]*$`.
    AdapterId, what = "adapter id", rule = NAME_RULE, check = is_name
);

string_id!(
    /// A workflow's own `name:`, and the key the catalog files it under
    /// — what a `kind: workflow` node's `use:` names and what `yunta
    /// run` takes: `^[A-Za-z][A-Za-z0-9_-]*$`.
    WorkflowName, what = "workflow name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// A skill's name as a node's `skills:` lists it and a pack
    /// declares it: `^[A-Za-z][A-Za-z0-9_-]*$`.
    SkillName, what = "skill name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// An input's name — the key under `inputs:` that `{{inputs.<name>}}`
    /// resolves: `^[A-Za-z][A-Za-z0-9_-]*$`.
    InputName, what = "input name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// An MCP server's name — the key under `mcp_servers:` a node's
    /// `mcp:` refers to: `^[A-Za-z][A-Za-z0-9_-]*$`.
    McpServerName, what = "MCP server name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// A model's name as the adapter's CLI accepts it — one printable
    /// word, opaque to the engine.
    ModelName, what = "model name", rule = TOKEN_RULE, check = is_token
);

string_id!(
    /// The name `agent:` gives an agent — one printable word that resolves
    /// to a definition the run owns: the project's `.yunta/agents/<name>.md`,
    /// or one a pack declares, named `publisher/agent`.
    AgentName, what = "agent name", rule = TOKEN_RULE, check = is_token
);

string_id!(
    /// A mode's name — a key under `modes:`, or `default` for a run of a
    /// workflow that declares none: `^[A-Za-z][A-Za-z0-9_-]*$`.
    ModeName, what = "mode name", rule = NAME_RULE, check = is_name
);

impl Default for ModeName {
    /// `default` — the mode a run of a workflow without `modes:` is
    /// recorded under.
    fn default() -> Self {
        ModeName::from_static("default")
    }
}

string_id!(
    /// A command the project declares under `commands:` and a workflow
    /// runs by name: `^[A-Za-z][A-Za-z0-9_-]*$`.
    CommandName, what = "command name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// The variable a directory under `shared_dirs:` is exported under,
    /// to every command and session of a run:
    /// `^[A-Z_][A-Z0-9_]*$`, and not `PATH`.
    SharedDirVar, what = "shared directory variable", rule = SHARED_DIR_VAR_RULE, check = is_shared_dir_var
);

string_id!(
    /// An executor's name — a `skills.executors` entry a `kind: executor`
    /// node refers to: `^[A-Za-z][A-Za-z0-9_-]*$`.
    ExecutorName, what = "executor name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// A question's id within a `kind: questions` artifact, and the key
    /// its answer names: `^[A-Za-z][A-Za-z0-9_-]*$`.
    QuestionId, what = "question id", rule = NAME_RULE, check = is_name
);

string_id!(
    /// One choice of a gate's escalation: an author's `options:` entry,
    /// a key of `on:`, or one of the engine's own reserved options
    /// (`abort`, `retry`, ...): `^[A-Za-z][A-Za-z0-9_-]*$`.
    OptionId, what = "option id", rule = NAME_RULE, check = is_name
);

string_id!(
    /// Who a human decision is attributed to — a claimed identity
    /// recorded verbatim, or an ambient one marked `unverified:`. Opaque
    /// to the engine beyond being one non-empty line.
    Responder, what = "responder", rule = OPAQUE_RULE, check = is_opaque
);

string_id!(
    /// An adapter session's id — opaque to the engine, persisted for
    /// `resume`.
    SessionId, what = "session id", rule = OPAQUE_RULE, check = is_opaque
);

string_id!(
    /// A pack publisher — the directory a publisher's packs vendor under
    /// and what `permissions.packs.publishers` matches on.
    Publisher, what = "publisher", rule = SEGMENT_RULE, check = is_segment
);

string_id!(
    /// A pack's own name — its directory under its publisher's.
    PackName, what = "pack name", rule = SEGMENT_RULE, check = is_segment
);
