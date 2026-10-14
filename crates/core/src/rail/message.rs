//! The sentences a session reads of a formatter's run.
//!
//! The data is [`FormatNote`]. Its prose lives here alone, so the engine
//! that records a note and the codec that delivers it never word one
//! themselves.

use std::fmt;

use super::answer::FormatNote;

impl fmt::Display for FormatNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatNote::Changed { run, paths } => {
                let names = paths
                    .iter()
                    .map(|path| format!("`{}`", path.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "yunta: `{run}` formatted {names} after your write; \
                     read it again before you edit it."
                )
            }
            FormatNote::Failed {
                run,
                exit_code,
                printed,
            } => write!(
                f,
                "yunta: `{run}` could not format what you wrote (exit {exit_code}):\n{}",
                printed.join("\n")
            ),
            FormatNote::TimedOut { run, after_ms } => write!(
                f,
                "yunta: `{run}` did not finish formatting what you wrote within {after_ms} ms, \
                 so it was stopped."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandName;
    use std::path::PathBuf;

    #[test]
    fn each_format_note_reads_as_the_session_is_told_it() {
        let run = CommandName::from_static("fmt");
        let changed = FormatNote::Changed {
            run: run.clone(),
            paths: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
        };
        assert_eq!(
            changed.to_string(),
            "yunta: `fmt` formatted `a.rs`, `b.rs` after your write; read it again before you edit it."
        );
        let failed = FormatNote::Failed {
            run: run.clone(),
            exit_code: 2,
            printed: vec!["bad line".to_string(), "worse line".to_string()],
        };
        assert_eq!(
            failed.to_string(),
            "yunta: `fmt` could not format what you wrote (exit 2):\nbad line\nworse line"
        );
        let timed_out = FormatNote::TimedOut {
            run,
            after_ms: 5_000,
        };
        assert_eq!(
            timed_out.to_string(),
            "yunta: `fmt` did not finish formatting what you wrote within 5000 ms, so it was stopped."
        );
    }
}
