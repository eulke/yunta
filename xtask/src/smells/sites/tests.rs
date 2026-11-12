//! Tests of the counters that read where a capability is written.

use super::*;
use crate::smells::corpus::scratch_tree;

/// The process is read once, at the shell: its directory, its binary, its
/// arguments or its signals read anywhere else count, and the shell and the
/// tests do not. A child process's directory, set on its command, is no read.
#[test]
fn the_process_read_below_the_shell_counts_and_the_shell_does_not() {
    let root = scratch_tree(
        "process-read",
        &[
            (
                "crates/cli/src/shell.rs",
                "let dir = std::env::current_dir()?;\nlet binary = std::env::current_exe();\n",
            ),
            (
                "crates/api/src/workspace.rs",
                "let dir = std::env::current_dir()?;\n",
            ),
            ("crates/engine/src/spawn.rs", "command.current_dir(&cwd);\n"),
            (
                "crates/cli/src/interrupt.rs",
                "let signals = tokio::signal::unix::signal(kind)?;\n",
            ),
            (
                "crates/cli/tests/flow.rs",
                "let here = std::env::current_dir()?;\n",
            ),
        ],
    );
    assert_eq!(count_process_read_below_the_shell(&root), 2);
}

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

/// Each spelling an escape is written with in source is read by a mark of
/// `ESCAPES`, and a char literal that names the escape byte is not.
#[test]
fn an_escape_counts_in_each_of_its_spellings() {
    let reads = |line: &str| ESCAPES.iter().any(|mark| mark.on(line));
    // Raw strings keep the backslashes, so each line holds its spelling: an
    // introducer, the escape byte, and an operating system command.
    assert!(reads(r#"        "\x1b[{sgr}m{text}\x1b[0m""#));
    assert!(reads(r#"    let reset = "\u{1b}[0m";"#));
    assert!(reads(r#"    "\x1b]8;;{target}\x1b\\""#));
    assert!(!reads(r#"    assert!(painted.contains('\x1b'));"#));
}

/// A key press writes the escape byte alone, so a string holding that byte
/// counts, while the char literal that names it in a check does not.
#[test]
fn a_lone_escape_byte_in_a_string_counts_and_its_char_literal_does_not() {
    let reads = |line: &str| ESCAPES.iter().any(|mark| mark.on(line));
    assert!(reads(r#"    terminal.keys("\x1b");"#));
    assert!(!reads(r#"    assert!(!line.contains('\x1b'));"#));
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
