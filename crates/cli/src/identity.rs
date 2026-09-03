//! Who a decision is attributed to in the audit trail.

/// The identity to stamp on a human decision — `gate_resolved.resolved_by`
/// and `questions_answered.responder`. An explicit `--by` is the
/// responder's own claim, recorded verbatim; without one, the shell's
/// `$USER` stands in, marked `unverified:` because nothing here
/// authenticates it. The log stays honest about which identities were
/// asserted and which were merely ambient. `$USER` unset degrades to
/// `unverified:unknown` rather than dropping the attribution entirely.
pub(crate) fn responder(claimed: Option<&str>) -> String {
    match claimed {
        Some(by) => by.to_string(),
        None => format!(
            "unverified:{}",
            std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::responder;

    #[test]
    fn an_explicit_claim_is_recorded_verbatim() {
        assert_eq!(responder(Some("alice")), "alice");
    }

    #[test]
    fn an_ambient_user_is_marked_unverified() {
        // Whatever `$USER` is, a decision made without `--by` carries the
        // `unverified:` mark so the log never presents an ambient identity
        // as a claimed one.
        assert!(
            responder(None).starts_with("unverified:"),
            "got: {}",
            responder(None)
        );
    }
}
