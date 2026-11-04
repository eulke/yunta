//! What a session does next while a request for scope it made waits.

/// What a session is told to do next while its request waits: a person
/// decides it when the attempt ends, and until then the work stays within the
/// scope it has; with no person to decide, the attempt is simply retried.
pub(crate) fn request_next(answered_by_a_person: bool, check_scope: &str) -> String {
    match answered_by_a_person {
        true => format!(
            "a person decides it when this attempt ends; until then, finish what you can within \
             the scope you have, without writing the paths you asked for; `{check_scope}` tells \
             you whether what you changed is within it"
        ),
        false => {
            "end this attempt; the work is attempted again once the request is decided".to_string()
        }
    }
}
