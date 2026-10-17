//! The words a tool call's target is named by: what it names when the
//! engine can vouch for it, and its shortened hash when it cannot.

use yunta_core::events::ToolTarget;

/// The one line a surface shows for a target: what it names when it can
/// be named, and the shortened hash when it cannot.
pub fn line(target: &ToolTarget) -> String {
    match target.display() {
        Some(display) => display.to_string(),
        None => target.digest().abbreviated(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn a_path_the_engine_can_name_reads_as_written() {
        let target = ToolTarget::of_path(Path::new("crates/core/src/lib.rs"));
        assert_eq!(line(&target), "crates/core/src/lib.rs");
    }

    #[test]
    fn a_target_the_session_wrote_reads_as_its_shortened_hash() {
        // The SHA-256 of the empty input, shortened to its first twelve digits.
        assert_eq!(line(&ToolTarget::opaque(b"")), "sha256:e3b0c44298fc");
    }
}
