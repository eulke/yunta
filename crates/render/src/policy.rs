//! The sentence a reader meets for each fallback the engine takes when an
//! adapter does not declare a capability the engine asked for.
//!
//! The log stores the closed [`Policy`] that was applied. These are the
//! words every surface prints for it, and for a log that stored the
//! sentence instead, the sentence as it was written.

use yunta_core::port::{Policy, PolicyApplied};

/// What the engine did instead, in the words every surface prints.
pub fn sentence(policy: &Policy) -> String {
    let said = match policy {
        Policy::PostCheckOnly => {
            "post-check only — the session edits unguarded and its diff is judged \
             against the declared scope after the fact"
        }
        Policy::NoTokenBudget => {
            "no token budget — the session reports no usage, so the run's cap is \
             recorded against it as zero"
        }
        Policy::NoSkills => {
            "skills not mounted — the session runs without the skills the node \
             declares"
        }
        Policy::NoRunTools => {
            "run tools unreachable — the session holds none of the run's tools, so \
             this node ends owing every document it declares"
        }
        Policy::NetworkOpen => {
            "declarative only — `network: false` is recorded for policy and audit, \
             not enforced"
        }
        Policy::FreshSession => "restart_node — a fresh session replaces the interrupted one",
        Policy::CapAtSessionEnd => {
            "the turn's token cap is checked when the session ends, because the CLI reports \
             no usage mid-session"
        }
    };
    said.to_string()
}

/// What a degradation's record says the engine did: a closed policy as its
/// sentence, or the sentence an older log stored, as it was written.
pub fn applied(applied: &PolicyApplied) -> String {
    match applied {
        PolicyApplied::Known(policy) => sentence(policy),
        PolicyApplied::Recorded(text) => text.as_str().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fallback_reads_as_what_the_engine_did_instead() {
        assert_eq!(
            sentence(&Policy::FreshSession),
            "restart_node — a fresh session replaces the interrupted one"
        );
        assert_eq!(
            sentence(&Policy::NoRunTools),
            "run tools unreachable — the session holds none of the run's tools, so this node \
             ends owing every document it declares"
        );
    }

    #[test]
    fn a_token_cap_checked_at_session_end_reads_as_when_it_is_checked() {
        assert_eq!(
            sentence(&Policy::CapAtSessionEnd),
            "the turn's token cap is checked when the session ends, because the CLI reports no \
             usage mid-session"
        );
    }

    #[test]
    fn a_policy_read_from_its_name_reads_as_its_sentence() {
        assert_eq!(
            applied(&PolicyApplied::Known(Policy::NoSkills)),
            "skills not mounted — the session runs without the skills the node declares"
        );
    }

    #[test]
    fn a_sentence_an_older_log_stored_reads_as_it_was_written() {
        let stored =
            PolicyApplied::Recorded("post-check only — the session edits unguarded".into());
        assert_eq!(
            applied(&stored),
            "post-check only — the session edits unguarded"
        );
    }
}
