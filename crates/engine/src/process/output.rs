//! What a governed command printed, as the run keeps and quotes it.

use super::Outcome;

/// What a command printed: its stdout, then its stderr, as one text a
/// reader reads top to bottom. Shared, so the cycle that ran the command,
/// the event that records it and the verdict a session reads hold one
/// copy between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    bytes: std::sync::Arc<[u8]>,
    /// Where its stdout ends and its stderr begins.
    stdout: usize,
}

impl CommandOutput {
    /// What `outcome`'s collected streams hold.
    pub fn of(outcome: &Outcome) -> Self {
        let (Outcome::Exited { stdout, stderr, .. }
        | Outcome::TimedOut { stdout, stderr, .. }
        | Outcome::Cancelled { stdout, stderr, .. }) = outcome;
        let mut bytes = stdout.clone();
        if !stdout.is_empty() && !stdout.ends_with(b"\n") && !stderr.is_empty() {
            bytes.push(b'\n');
        }
        let stdout = bytes.len();
        bytes.extend_from_slice(stderr);
        CommandOutput {
            bytes: bytes.into(),
            stdout,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Its stdout and its stderr, apart, as the command wrote them.
    fn streams(&self) -> (&[u8], &[u8]) {
        self.bytes.split_at(self.stdout)
    }

    /// The last [`TAIL_LINES`](yunta_core::events::TAIL_LINES) lines, in
    /// order.
    pub fn tail(&self) -> Vec<String> {
        let text = String::from_utf8_lossy(&self.bytes);
        let mut tail: Vec<String> = text
            .lines()
            .rev()
            .take(yunta_core::events::TAIL_LINES)
            .map(str::to_string)
            .collect();
        tail.reverse();
        tail
    }
}

/// What a command printed, as a check holds it: everything it printed in
/// this invocation, or what an earlier invocation's log kept of it — the
/// object that holds it, when one was kept, its last lines, and the line
/// it concluded with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Printed {
    Ran(CommandOutput),
    Recorded {
        object: Option<yunta_core::ContentHash>,
        tail: Vec<String>,
        /// The line the command concluded with, as the log recorded it.
        /// `None` on a log that kept only the tail.
        conclusion: Option<String>,
    },
}

impl Printed {
    /// The line it concluded with: the last non-empty line on stdout, else
    /// on stderr. Read from the whole output when it is at hand, and from
    /// what the log kept of it otherwise.
    pub fn concluded(&self) -> Option<String> {
        match self {
            Printed::Ran(output) => {
                let (stdout, stderr) = output.streams();
                conclusion(stdout, stderr)
            }
            Printed::Recorded {
                conclusion: kept,
                tail,
                ..
            } => kept.clone().or_else(|| legacy_conclusion(tail)),
        }
    }

    /// Its last lines, in order.
    pub fn tail(&self) -> Vec<String> {
        match self {
            Printed::Ran(output) => output.tail(),
            Printed::Recorded { tail, .. } => tail.clone(),
        }
    }
}

/// The line a command concluded with: the last non-empty line on stdout,
/// else on stderr. A test runner says what it ran on stdout, and its
/// progress on stderr would otherwise bury that, so stdout comes first.
/// Computed once, when the output is recorded, and read back from there.
pub fn conclusion(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    last_line(stdout).or_else(|| last_line(stderr))
}

/// The conclusion of a recorded output whose log kept no conclusion: the
/// last non-empty line of the tail it kept.
pub fn legacy_conclusion(tail: &[String]) -> Option<String> {
    last_line(tail.join("\n").as_bytes())
}

/// The last non-empty line of `bytes`, cut to a line's worth of
/// characters.
fn last_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?
        .trim();
    Some(line.chars().take(240).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{Drain, Outcome};

    /// A command whose conclusion the log kept concludes as it did live,
    /// though the tail it also kept mixes the two streams.
    #[test]
    fn a_replayed_command_concludes_as_it_did_live() {
        let status = std::process::Command::new("true").status().unwrap();
        let out = Outcome::Exited {
            status,
            stdout: b"ok\n".to_vec(),
            stderr: b"warn: slow\n".to_vec(),
            drain: Drain::Complete,
        };
        let live = Printed::Ran(CommandOutput::of(&out));
        let replayed = Printed::Recorded {
            object: None,
            tail: live.tail(),
            conclusion: live.concluded(),
        };
        assert_eq!(live.concluded().as_deref(), Some("ok"));
        assert_eq!(replayed.concluded().as_deref(), Some("ok"));
    }
}
