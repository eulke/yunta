use clap::Parser;
use yunta_core::step::samples::every_step;
use yunta_core::step::NextStep;
use yunta_render::command::{line_for, Reader};

use super::Cli;

/// `line` as a reader types it: each blank, `<option>` or `"<answer>"`, filled
/// with a value every blank accepts, since a blank is not meant to be typed as it
/// is written.
fn filled(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|word| match word.trim_matches('"').starts_with('<') {
            true => "retry".to_string(),
            false => word.to_string(),
        })
        .collect()
}

/// Every line a surface writes is one the CLI accepts once its blanks are
/// filled, for a person and for an agent: a renamed subcommand or flag fails
/// here rather than on a reader's terminal. `git show` is git's, and is held to
/// its own form.
#[test]
fn every_command_line_parses() {
    for step in &every_step() {
        for reader in [Reader::Person, Reader::Agent] {
            let line = line_for(step, reader);
            if let NextStep::ShowCommit { commit } = step {
                assert_eq!(line, format!("git show {commit}"));
                continue;
            }
            let parsed = Cli::try_parse_from(filled(&line));
            let accepted = match &parsed {
                Ok(_) => true,
                Err(error) => error.kind() == clap::error::ErrorKind::DisplayHelp,
            };
            assert!(
                accepted,
                "`{line}` is not a command line yunta reads: {}",
                parsed.err().map(|e| e.to_string()).unwrap_or_default()
            );
        }
    }
}
