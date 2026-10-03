//! Reading a subprocess pipe one line at a time.
//!
//! A CLI writes its protocol as lines, and a reader of those lines is
//! reading bytes a process this one does not control chose: they may not
//! be UTF-8, and there may be no line ending in sight. Both are bounded
//! here — decoded with replacement characters, cut off at
//! [`MAX_LINE_BYTES`] — so no writer can grow this process's memory
//! without end and no byte sequence can stop the read.

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use super::subprocess::MAX_LINE_BYTES;

/// Lines from a pipe: decoded with replacement characters where the
/// bytes are not UTF-8, without their line ending, and at most
/// [`MAX_LINE_BYTES`] long — a longer one is discarded whole, with a
/// warning, and reading goes on with the next. A last line without a
/// line ending is still a line.
pub(super) struct LineReader<R> {
    reader: BufReader<R>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    pub(super) fn new(pipe: R) -> Self {
        LineReader {
            reader: BufReader::new(pipe),
        }
    }

    pub(super) async fn next_line(&mut self) -> Option<String> {
        let mut line = Vec::new();
        let mut discarding = false;
        loop {
            let available = match self.reader.fill_buf().await {
                Ok(available) => available,
                Err(e) => {
                    tracing::warn!(error = %e, "error reading a subprocess pipe");
                    return None;
                }
            };
            if available.is_empty() {
                return (!discarding && !line.is_empty()).then(|| decode(&line));
            }
            let (chunk, ended) = match available
                .iter()
                .position(|byte| *byte == b'\n')
                .and_then(|at| available.get(..at))
            {
                Some(chunk) => (chunk, true),
                None => (available, false),
            };
            let consumed = chunk.len() + usize::from(ended);
            if !discarding && line.len() + chunk.len() > MAX_LINE_BYTES {
                tracing::warn!(
                    limit = MAX_LINE_BYTES,
                    "discarding a subprocess line longer than the limit"
                );
                line.clear();
                discarding = true;
            } else if !discarding {
                line.extend_from_slice(chunk);
            }
            self.reader.consume(consumed);
            if ended {
                if discarding {
                    discarding = false;
                    continue;
                }
                return Some(decode(&line));
            }
        }
    }
}

fn decode(line: &[u8]) -> String {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines_of(bytes: &'static [u8]) -> Vec<String> {
        let mut reader = LineReader::new(bytes);
        let mut lines = Vec::new();
        while let Some(line) = reader.next_line().await {
            lines.push(line);
        }
        lines
    }

    #[tokio::test]
    async fn bytes_that_are_not_utf8_become_replacement_characters() {
        assert_eq!(
            lines_of(b"caf\xff\xfe\nok\n").await,
            vec!["caf\u{FFFD}\u{FFFD}".to_string(), "ok".to_string()]
        );
    }

    #[tokio::test]
    async fn a_last_line_without_a_line_ending_is_still_a_line() {
        assert_eq!(
            lines_of(b"first\r\nlast").await,
            vec!["first".to_string(), "last".to_string()]
        );
    }

    #[tokio::test]
    async fn a_line_over_the_limit_is_discarded_and_reading_goes_on() {
        let oversized: &'static [u8] = Box::leak(
            [vec![b'x'; MAX_LINE_BYTES + 1], b"\nafter\n".to_vec()]
                .concat()
                .into_boxed_slice(),
        );
        assert_eq!(lines_of(oversized).await, vec!["after".to_string()]);
    }
}
