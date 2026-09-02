use std::fmt;

use serde::{Deserialize, Serialize};

/// Defines a newtyped string identifier with the shared boilerplate
/// (Display, From<&str>/From<String>, as_str). CLAUDE.md: "Newtypes para
/// todo identificador — nunca `String` pelada." Validation of an
/// identifier's expected shape (e.g. the ledger's `^[A-Za-z][...]*$`
/// pattern) belongs to whoever parses it into existence, not to
/// the newtype itself — an id here can be any string, just never
/// interchangeable with a different kind of id by accident.
macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_string())
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
    };
}

string_id!(
    /// A node identifier, unique within a workflow.
    NodeId
);

string_id!(
    /// An adapter's id as config names it (`claude-code`, `codex`,
    /// `mock`).
    AdapterId
);

string_id!(
    /// A run identifier (a ULID). Nothing generates real ULIDs
    /// yet — that comes later, via an injected id source per
    /// CLAUDE.md's "determinismo inyectado" — this type only fixes the
    /// domain shape ahead of that.
    RunId
);

string_id!(
    /// A task identifier from the ledger (`^[A-Za-z][A-Za-z0-9_-]*$`,
    /// unique within one ledger).
    TaskId
);

string_id!(
    /// An adapter session identifier — opaque to the engine, persisted
    /// for `resume`.
    SessionId
);
