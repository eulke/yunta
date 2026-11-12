//! Workflows that tests in more than one file run, each with what it is for.

/// A check that fails in either mode, its reroute spent at once, so the decision it parks
/// on offers `promote`: `quick` promotes to `full`, which has nowhere later to go.
pub const PROMOTABLE: &str = r#"
name: promotable
modes:
  quick: { include: [check, fix] }
  full:  { include: all }
nodes:
  - id: check
    kind: bash
    run: "test -f fixed.txt"
    on_failure: { goto: fix, max_reroutes: 0 }
  - id: fix
    kind: bash
    run: "touch fixed.txt"
"#;
