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

    /// The last non-empty line, cut to a line's worth of characters:
    /// enough to name what failed, never a whole build log.
    pub fn last_words(&self) -> Option<String> {
        last_line(&self.bytes)
    }

    /// The last non-empty line it printed on stdout, or on stderr when
    /// stdout said nothing: where a test runner says what it ran, which
    /// its progress on stderr would otherwise bury.
    pub fn concluded(&self) -> Option<String> {
        self.bytes
            .get(..self.stdout)
            .and_then(last_line)
            .or_else(|| self.last_words())
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
/// object that holds it, when one was kept, and its last lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Printed {
    Ran(CommandOutput),
    Recorded {
        object: Option<yunta_core::ContentHash>,
        tail: Vec<String>,
    },
}

impl Printed {
    /// The last non-empty line, cut to a line's worth of characters.
    pub fn last_words(&self) -> Option<String> {
        match self {
            Printed::Ran(output) => output.last_words(),
            Printed::Recorded { tail, .. } => last_line(tail.join("\n").as_bytes()),
        }
    }

    /// The line it concluded with: the last on stdout when the whole
    /// output is at hand, the last of what was kept otherwise.
    pub fn concluded(&self) -> Option<String> {
        match self {
            Printed::Ran(output) => output.concluded(),
            Printed::Recorded { .. } => self.last_words(),
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
