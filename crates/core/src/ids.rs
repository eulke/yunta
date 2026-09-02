//! Newtyped identifiers, parsed once at the frontier.
//!
//! Every identifier checks its rule when it is built from text — through
//! `FromStr`, `TryFrom` or `Deserialize` — so an invalid value is
//! unrepresentable past the parser that read it, and two kinds of id can
//! never be swapped by accident. The rules are the ones the schema and
//! the ledger document; each type's rustdoc states its own.
//!
//! Unchecked construction from a literal (`From<&str>`) exists only for
//! tests, behind the `testkit` feature; production code parses through
//! `FromStr` or `TryFrom<String>`.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

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
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        let byte = bytes[i];
        if !(byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
            return false;
        }
        i += 1;
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
    while i < bytes.len() {
        if bytes[i] == FAN_OUT_SEPARATOR {
            let (base, rest) = bytes.split_at(i);
            let (_, runner) = rest.split_at(1);
            return is_name(base) && is_name(runner);
        }
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
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_graphic() {
            return false;
        }
        i += 1;
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
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' || bytes[i] == b'\\' {
            return false;
        }
        i += 1;
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
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] < 0x20 || bytes[i] == 0x7F {
            return false;
        }
        i += 1;
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

string_id!(
    /// A node's id, unique within a workflow: `^[A-Za-z][A-Za-z0-9_-]*$`
    /// as authored. A fan-out sibling — one of the nodes the manifest
    /// expands a `runners:` node into — is `<base>@<runner>`; authored
    /// YAML never spells that form, the manifest does.
    NodeId, what = "node id", rule = NODE_RULE, check = is_node_id
);

impl NodeId {
    /// The id of the sibling of `base` that runs under `runner`. `base`
    /// contributes its authored id, so the result is one `@` deep even
    /// when `base` is itself a sibling.
    pub fn fan_out(base: &NodeId, runner: &RunnerName) -> NodeId {
        NodeId(Cow::Owned(format!(
            "{}{}{runner}",
            base.base_str(),
            FAN_OUT_SEPARATOR as char
        )))
    }

    /// Whether this is a fan-out sibling (`<base>@<runner>`).
    pub fn is_fan_out(&self) -> bool {
        self.0.contains(FAN_OUT_SEPARATOR as char)
    }

    /// The authored id: the part before `@` for a sibling, the id itself
    /// otherwise.
    pub fn base(&self) -> NodeId {
        if self.is_fan_out() {
            NodeId(Cow::Owned(self.base_str().to_string()))
        } else {
            self.clone()
        }
    }

    /// The runner a fan-out sibling runs under; `None` for an authored
    /// node.
    pub fn runner(&self) -> Option<RunnerName> {
        self.0
            .split_once(FAN_OUT_SEPARATOR as char)
            .map(|(_, runner)| RunnerName(Cow::Owned(runner.to_string())))
    }

    fn base_str(&self) -> &str {
        self.0
            .split_once(FAN_OUT_SEPARATOR as char)
            .map_or(self.as_str(), |(base, _)| base)
    }
}

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
    /// A model's name as the adapter's CLI accepts it — one printable
    /// word, opaque to the engine.
    ModelName, what = "model name", rule = TOKEN_RULE, check = is_token
);

string_id!(
    /// A named agent of an adapter (`agent:`) — one printable word,
    /// opaque to the engine.
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
    /// An executor's name — a `skills.executors` entry a `kind: executor`
    /// node refers to: `^[A-Za-z][A-Za-z0-9_-]*$`.
    ExecutorName, what = "executor name", rule = NAME_RULE, check = is_name
);

string_id!(
    /// A run's id — the name of its directory under the runs root.
    RunId, what = "run id", rule = SEGMENT_RULE, check = is_segment
);

string_id!(
    /// A task's id from the ledger, unique within one ledger:
    /// `^[A-Za-z][A-Za-z0-9_-]*$`.
    TaskId, what = "task id", rule = NAME_RULE, check = is_name
);

string_id!(
    /// A finding's id within a `kind: findings` artifact — one printable
    /// label, unique per author. The engine's own findings compose theirs
    /// from the node or artifact they concern.
    FindingId, what = "finding id", rule = OPAQUE_RULE, check = is_opaque
);

string_id!(
    /// A question's id within a `kind: questions` artifact, and the key
    /// its answer names: `^[A-Za-z][A-Za-z0-9_-]*$`.
    QuestionId, what = "question id", rule = NAME_RULE, check = is_name
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

// --- Composite identifiers ---------------------------------------------

/// A pack's identity, `publisher/name`: what `yunta run acme/review`
/// and `use: acme/qa-review` name, and the key of `yunta.lock`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackRef {
    publisher: Publisher,
    name: PackName,
}

const PACK_REF_RULE: &str = "`publisher/name`, each one path segment";

impl PackRef {
    pub fn new(publisher: Publisher, name: PackName) -> Self {
        PackRef { publisher, name }
    }

    pub fn publisher(&self) -> &Publisher {
        &self.publisher
    }

    pub fn name(&self) -> &PackName {
        &self.name
    }
}

impl fmt::Display for PackRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.publisher, self.name)
    }
}

impl FromStr for PackRef {
    type Err = InvalidId;

    fn from_str(value: &str) -> Result<Self, InvalidId> {
        let invalid = || InvalidId {
            what: "pack reference",
            value: value.to_string(),
            rule: PACK_REF_RULE,
        };
        let (publisher, name) = value.split_once('/').ok_or_else(invalid)?;
        Ok(PackRef {
            publisher: publisher.parse().map_err(|_| invalid())?,
            name: name.parse().map_err(|_| invalid())?,
        })
    }
}

impl TryFrom<String> for PackRef {
    type Error = InvalidId;

    fn try_from(value: String) -> Result<Self, InvalidId> {
        value.parse()
    }
}

impl Serialize for PackRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PackRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

// --- Process ids ---------------------------------------------------------

/// A process id: a positive number, as the kernel hands them out. Zero
/// and negatives — what a signal call or a lock file could otherwise
/// carry by mistake — are unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(u32);

const PID_RULE: &str = "a positive number";

impl Pid {
    /// This process's own id. The kernel never runs a user process as
    /// pid 0, so no check is needed here.
    pub fn current() -> Pid {
        Pid(std::process::id())
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<u32> for Pid {
    type Error = InvalidId;

    fn try_from(value: u32) -> Result<Self, InvalidId> {
        if value == 0 {
            return Err(InvalidId {
                what: "process id",
                value: value.to_string(),
                rule: PID_RULE,
            });
        }
        Ok(Pid(value))
    }
}

impl TryFrom<i32> for Pid {
    type Error = InvalidId;

    fn try_from(value: i32) -> Result<Self, InvalidId> {
        u32::try_from(value)
            .ok()
            .filter(|raw| *raw > 0)
            .map(Pid)
            .ok_or_else(|| InvalidId {
                what: "process id",
                value: value.to_string(),
                rule: PID_RULE,
            })
    }
}

impl Serialize for Pid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for Pid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        Pid::try_from(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_starts_with_a_letter_and_continues_with_letters_digits_underscore_or_dash() {
        assert!(is_name(b"a"));
        assert!(is_name(b"review-alt_2"));
        assert!(!is_name(b""));
        assert!(!is_name(b"2fast"));
        assert!(!is_name(b"a.b"));
        assert!(!is_name(b"a b"));
    }

    #[test]
    fn a_node_id_admits_exactly_one_fan_out_separator_between_two_names() {
        assert!(is_node_id(b"review"));
        assert!(is_node_id(b"review@alt"));
        assert!(!is_node_id(b"review@"));
        assert!(!is_node_id(b"@alt"));
        assert!(!is_node_id(b"a@b@c"));
    }

    #[test]
    fn a_segment_is_a_printable_word_that_names_one_directory() {
        assert!(is_segment(b"acme"));
        assert!(is_segment(b"run-1"));
        assert!(!is_segment(b"."));
        assert!(!is_segment(b".."));
        assert!(!is_segment(b"a/b"));
        assert!(!is_segment(b"a\\b"));
        assert!(!is_segment(b"a b"));
        assert!(!is_segment(b""));
    }

    #[test]
    fn a_static_identifier_borrows_and_a_parsed_one_owns_but_they_compare_equal() {
        static STATIC: AdapterId = AdapterId::from_static("codex");
        let parsed: AdapterId = "codex".parse().unwrap();
        assert_eq!(STATIC, parsed);
        let mut set = std::collections::HashSet::new();
        set.insert(parsed);
        assert!(set.contains(&STATIC));
        assert!(set.contains("codex"));
    }

    #[test]
    fn debug_names_the_type_around_the_value() {
        let id: NodeId = "plan".parse().unwrap();
        assert_eq!(format!("{id:?}"), "NodeId(\"plan\")");
    }
}
