//! How much a change is, as git counts it: `diff --numstat` from one commit
//! or tree to another, a renamed file counted once and a binary file as a
//! file with no lines.

use std::path::Path;

use crate::process::Supervision;

use super::{read_bytes, GitError};

/// A change as `diff --numstat` counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Numstat {
    /// How many files the change touches.
    pub files: u32,
    /// How many lines it adds.
    pub added: u64,
    /// How many lines it removes.
    pub removed: u64,
}

/// The change from `from` to `to`, two commits or trees of the repository
/// `dir` is in, with renames found whatever the repository's own
/// configuration says, so a renamed file counts once.
pub async fn numstat(
    dir: &Path,
    from: &str,
    to: &str,
    sup: Supervision<'_>,
) -> Result<Numstat, GitError> {
    let args = ["diff", "--numstat", "-z", "--find-renames", from, to];
    let printed = read_bytes(dir, &args, sup).await?;
    Ok(counted(&printed))
}

/// What `diff --numstat -z` printed, counted. Each record is the lines
/// added, a tab, the lines removed, a tab, and the path ending in a NUL; a
/// rename writes no path there, and its old and new paths follow, each
/// ending in a NUL. A binary file prints `-` for both counts.
fn counted(printed: &[u8]) -> Numstat {
    let mut stat = Numstat::default();
    let mut fields = printed.split(|&byte| byte == 0);
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, |&byte| byte == b'\t');
        let added = parts.next().and_then(lines);
        let removed = parts.next().and_then(lines);
        if parts.next().unwrap_or_default().is_empty() {
            // A rename: its old and new paths are the next two fields.
            fields.next();
            fields.next();
        }
        stat.files += 1;
        stat.added += added.unwrap_or(0);
        stat.removed += removed.unwrap_or(0);
    }
    stat
}

/// A count of lines as numstat prints it; `None` for a binary file's `-`.
fn lines(field: &[u8]) -> Option<u64> {
    std::str::from_utf8(field).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each record counts once: a file with its lines, a binary file's
    /// dashes as no lines, and a rename's two paths as one file.
    #[test]
    fn counted_reads_each_record_once() {
        let printed = [
            &b"3\t1\tsrc/a.rs\0"[..],
            b"-\t-\tlogo.png\0",
            b"0\t0\t\0old name.rs\0new name.rs\0",
        ]
        .concat();
        assert_eq!(
            counted(&printed),
            Numstat {
                files: 3,
                added: 3,
                removed: 1,
            }
        );
    }

    /// A change of nothing prints nothing and counts nothing.
    #[test]
    fn nothing_printed_counts_nothing() {
        assert_eq!(counted(b""), Numstat::default());
    }
}
