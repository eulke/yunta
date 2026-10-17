//! What the project's permissions refused a command for: the command, and the
//! list that refused it, or the sentence an older log stored.

use serde::{Deserialize, Serialize};

use crate::events::LoggedWords;

/// What the project's permissions refused: the command, and the pattern of
/// the list that refused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeniedCommand {
    pub command: String,
    pub by: DeniedBy,
}

/// The list of `permissions.commands` that refused a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "list", rename_all = "snake_case")]
pub enum DeniedBy {
    /// `permissions.commands.deny` holds a pattern the command matches.
    Deny { pattern: String },
    /// `permissions.commands.allow` is a strict allowlist, and no pattern matches.
    Allow,
}

/// What a denial records: the command, or the sentence an older log stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum DeniedRule {
    Command(DeniedCommand),
    /// The sentence an older log stored, kept as written.
    Logged(LoggedWords),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn denied_marker() -> DeniedCommand {
        DeniedCommand {
            command: "echo forbidden-marker".into(),
            by: DeniedBy::Deny {
                pattern: "*forbidden-marker*".into(),
            },
        }
    }

    #[test]
    fn a_denied_command_is_written_as_the_command_and_the_list_that_refused_it() {
        let rule = DeniedRule::Command(denied_marker());
        let written = serde_json::to_value(&rule).unwrap();
        assert_eq!(
            written,
            serde_json::json!({
                "command": "echo forbidden-marker",
                "by": { "list": "deny", "pattern": "*forbidden-marker*" },
            })
        );
        let read: DeniedRule = serde_json::from_value(written).unwrap();
        assert_eq!(read, rule);
    }

    #[test]
    fn a_refusal_by_an_allowlist_names_no_pattern() {
        let allowed = DeniedCommand {
            command: "rm -rf x".into(),
            by: DeniedBy::Allow,
        };
        assert_eq!(
            serde_json::to_value(&allowed).unwrap(),
            serde_json::json!({ "command": "rm -rf x", "by": { "list": "allow" } })
        );
    }

    #[test]
    fn a_sentence_an_older_log_stored_reads_as_logged() {
        let sentence =
            "command `rm -rf x` matches denied pattern `rm *` (permissions.commands.deny)";
        let read: DeniedRule = serde_json::from_value(serde_json::json!(sentence)).unwrap();
        assert_eq!(read, DeniedRule::Logged(sentence.into()));
    }
}
