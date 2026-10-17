//! Tests of the counters that read where a capability is written.

use super::*;

#[test]
fn a_text_mark_reads_the_spelling_the_code_writes() {
    // The counters that stand at zero stand there because the tree
    // holds none of their pattern, not because the pattern cannot
    // match: these are the lines each of them is waiting for.
    assert!(Text("env::set_var").on("    std::env::set_var(\"HOME\", home);"));
    assert!(Text("env::remove_var").on("    env::remove_var(\"YUNTA_HOME\");"));
    assert!(Text("fn event(").on("pub fn event(self, payload: EventPayload) -> Self {"));
    assert!(!Text("fn event(").on("    let event = log.event(payload);"));
    assert!(Text("} tok").on("        \"{INDENT}{}  {total:>8} tok\\n\","));
    assert!(Text("} tok").on("    \"CPTV: {cptv:.1} tokens/task\""));
    assert!(!Text("} tok").on("    \"tokens: {} in / {} out\""));
    assert!(Text("Command::new(\"git\"").on("    std::process::Command::new(\"git\")"));
    assert!(Text("Command::new(\"git\"").on("    GovernedCommand::new(\"git\", cwd)"));
}

#[test]
fn a_macro_mark_reads_the_name_and_not_the_longer_one_it_ends() {
    assert!(Macro("println!").on("    println!(\"{}\", value);"));
    assert!(!Macro("println!").on("    eprintln!(\"{}\", value);"));
    assert!(Macro("eprintln!").on("    eprintln!(\"{}\", value);"));
    assert!(!Macro("print!").on("    // print! is what a comment names"));
}

#[test]
fn a_struct_mark_reads_the_name_and_not_the_line_before_the_keyword() {
    assert!(StructNamed("Bench").on("pub struct Bench {"));
    assert!(StructNamed("Clock").on("    struct FixedClock;"));
    assert!(!StructNamed("Bench").on("let bench = Bench::new();"));
}

#[test]
fn an_owner_covers_a_directory_and_a_file_in_every_crate_a_star_names() {
    assert!(owned(
        Path::new("/w/crates/testkit/src/bench/mod.rs"),
        &["crates/testkit/"]
    ));
    assert!(!owned(
        Path::new("/w/crates/testkit-core/src/log.rs"),
        &["crates/testkit/"]
    ));
    // `crates/testkit*/src/repo.rs` names that file in either harness
    // crate, and nothing beside it.
    let pattern = ["crates/testkit*/src/repo.rs"];
    assert!(owned(Path::new("/w/crates/testkit/src/repo.rs"), &pattern));
    assert!(owned(
        Path::new("/w/crates/testkit-core/src/repo.rs"),
        &pattern
    ));
    assert!(!owned(Path::new("/w/crates/engine/src/repo.rs"), &pattern));
}

#[test]
fn a_cli_spelling_in_backticks_counts_and_a_comment_or_a_git_argument_does_not() {
    assert!(Spelled("`--").on("    \"pass `--input {name}=...` to name it\""));
    assert!(Spelled("`yunta ").on("    \"make room (`yunta gc`) and resume\","));
    // A comment explains, a git argument array has no backticks, a
    // delimiter is not a flag, and a schema title names no command.
    assert!(!Spelled("`--").on("    /// pass `--input` to name an input"));
    assert!(!Spelled("`--").on("    &[\"diff\", \"--name-only\", \"HEAD\"],"));
    assert!(!Spelled("`--").on("    \"start it with `---`, its `name`\""));
    assert!(!Spelled("`yunta ").on("    titled(schema_for!(Workflow), \"yunta workflow\")"));
}
