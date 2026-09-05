//! Who a decision is attributed to in the audit trail.

use yunta_core::Responder;

/// The identity to stamp on a human decision — `gate_resolved.resolved_by`
/// and `questions_answered.responder`. An explicit `--by` is the
/// responder's own claim, recorded verbatim; without one, the shell's
/// `$USER` stands in, marked `unverified:` because nothing here
/// authenticates it. The log stays honest about which identities were
/// asserted and which were merely ambient. `$USER` unset degrades to
/// `unverified:unknown` rather than dropping the attribution entirely.
pub(crate) fn responder(claimed: Option<&Responder>) -> Responder {
    match claimed {
        Some(by) => by.clone(),
        None => {
            let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
            // `$USER` is one word on every shell this runs under; a value
            // that is not one line falls back to the unknown ambient identity.
            format!("unverified:{user}")
                .parse()
                .unwrap_or_else(|_| Responder::from_static("unverified:unknown"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::responder;
    use yunta_core::Responder;

    #[test]
    fn an_explicit_claim_is_recorded_verbatim() {
        let alice: Responder = "alice".parse().unwrap();
        assert_eq!(responder(Some(&alice)), "alice");
    }

    #[test]
    fn an_ambient_user_is_marked_unverified() {
        // Whatever `$USER` is, a decision made without `--by` carries the
        // `unverified:` mark so the log never presents an ambient identity
        // as a claimed one.
        assert!(
            responder(None).as_str().starts_with("unverified:"),
            "got: {}",
            responder(None)
        );
    }
}
