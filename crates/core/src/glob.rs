//! The one compilation rule for a scope glob — a node's `scope`, a
//! task's `scope`, an expansion's `within`, a session's edit
//! constraints. `*` never crosses a `/`; `**` is how a pattern asks for
//! recursion. A scope is a ceiling on what may change, and a ceiling
//! reads with the strictest interpretation its syntax allows.

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};

/// Compiles one scope pattern.
pub fn scope_glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).literal_separator(true).build()
}

/// Compiles a whole scope; `Err` carries the pattern that failed, or
/// the joined list when the set itself could not be built.
pub fn scope_globset(patterns: &[String]) -> Result<GlobSet, (String, globset::Error)> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(scope_glob(pattern).map_err(|error| (pattern.clone(), error))?);
    }
    builder
        .build()
        .map_err(|error| (patterns.join(", "), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_star_never_crosses_a_directory_and_a_double_star_does() {
        let set = scope_globset(&["src/*.rs".to_string()]).unwrap();
        assert!(set.is_match("src/lib.rs"));
        assert!(!set.is_match("src/deep/nested.rs"));
        let set = scope_globset(&["src/**".to_string()]).unwrap();
        assert!(set.is_match("src/deep/nested.rs"));
    }
}
