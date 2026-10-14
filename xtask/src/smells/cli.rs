//! The counters over what the command line says to a person: the status a
//! failure leaves through, and the text printed to the terminal.

use std::path::Path;

use super::corpus::{is_test_source, rs_files};
use super::read_corpus;
use super::shape::production_only;
use super::sites::Mark::{Macro, Text};
use super::sites::{outside, owned};

/// A failure's exit status spelled outside the one place the CLI maps it.
///
/// `main` owns the status of every outcome, so a second spelling of the
/// failure status can drift from the one its translator gives.
pub(super) fn count_exit_failure_in_cli(root: &Path) -> usize {
    outside(
        root,
        &[Text("ExitCode::FAILURE")],
        &["crates/cli/src/main.rs"],
    )
}

/// Printing written outside the one file that owns the CLI's output.
///
/// Output has one owner, so every line a person reads goes through the same
/// width, color and error handling. A print anywhere else is text that
/// skips them. Only production code counts: a test module's output is not
/// what a person reads.
pub(super) fn count_cli_print_outside_output(root: &Path) -> usize {
    const OWNERS: [&str; 1] = ["crates/cli/src/output.rs"];
    let marks = [
        Macro("print!"),
        Macro("println!"),
        Macro("eprint!"),
        Macro("eprintln!"),
    ];
    rs_files(&root.join("crates/cli/src"))
        .iter()
        .filter(|path| !is_test_source(path) && !owned(path, &OWNERS))
        .map(|path| production_only(&read_corpus(path)))
        .map(|text| {
            text.lines()
                .filter(|line| marks.iter().any(|mark| mark.on(line)))
                .count()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smells::corpus::scratch_tree;

    #[test]
    fn only_the_output_file_may_print_and_a_print_anywhere_else_under_the_cli_counts() {
        let root = scratch_tree(
            "cli-print",
            &[
                (
                    "crates/cli/src/output.rs",
                    "pub fn say(text: &str) {\n    println!(\"{text}\");\n}\n",
                ),
                (
                    "crates/cli/src/error.rs",
                    "pub fn warn(text: &str) {\n    eprintln!(\"{text}\");\n}\n",
                ),
                (
                    "crates/cli/src/commands/status.rs",
                    "fn status() {\n    println!(\"ok\");\n}\n",
                ),
                (
                    "crates/cli/src/commands/tests.rs",
                    "fn check() {\n    println!(\"ok\");\n}\n",
                ),
                (
                    "crates/cli/src/commands/run.rs",
                    "fn run() {}\n#[cfg(test)]\nmod tests {\n    fn check() {\n        println!(\"ok\");\n    }\n}\n",
                ),
            ],
        );
        // The output file is the one owner, and a test file or a test module
        // prints for the test, not for a person: the two prints in `error.rs`
        // and `status.rs` are the only ones that count.
        assert_eq!(count_cli_print_outside_output(&root), 2);
    }
}
