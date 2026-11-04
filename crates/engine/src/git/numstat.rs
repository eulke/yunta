//! How much a change is, as git counts it: `diff --numstat` from one commit
//! or tree to another, a renamed file counted once and a binary file as a
//! file with no lines.

use std::path::Path;

use yunta_core::events::FileChange;

use crate::process::Supervision;

use super::{read_bytes, GitError};

/// How many files a change names by their paths: as many as a closing has room
/// for at a glance, the rest said as one sum.
const LARGEST_FILES: usize = 4;

/// A change as `diff --numstat` counts it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Numstat {
    /// How many files the change touches.
    pub files: u32,
    /// How many lines it adds.
    pub added: u64,
    /// How many lines it removes.
    pub removed: u64,
    /// The files that move most, the largest first.
    pub largest: Vec<FileChange>,
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
    let mut files: Vec<(u64, u64, &[u8])> = Vec::new();
    let mut fields = printed.split(|&byte| byte == 0);
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, |&byte| byte == b'\t');
        let added = parts.next().and_then(lines).unwrap_or(0);
        let removed = parts.next().and_then(lines).unwrap_or(0);
        let mut path = parts.next().unwrap_or_default();
        if path.is_empty() {
            // A rename: its old and new paths are the next two fields.
            fields.next();
            path = fields.next().unwrap_or_default();
        }
        stat.files += 1;
        stat.added += added;
        stat.removed += removed;
        files.push((added, removed, path));
    }
    files.sort_by(
        |(added, removed, path), (other_added, other_removed, other_path)| {
            (other_added + other_removed)
                .cmp(&(added + removed))
                .then_with(|| path.cmp(other_path))
        },
    );
    stat.largest = files
        .into_iter()
        .take(LARGEST_FILES)
        .map(|(added, removed, path)| FileChange {
            path: String::from_utf8_lossy(path).into_owned(),
            added,
            removed,
        })
        .collect();
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
                largest: vec![
                    FileChange {
                        path: "src/a.rs".into(),
                        added: 3,
                        removed: 1,
                    },
                    FileChange {
                        path: "logo.png".into(),
                        added: 0,
                        removed: 0,
                    },
                    FileChange {
                        path: "new name.rs".into(),
                        added: 0,
                        removed: 0,
                    },
                ],
            }
        );
    }

    /// A change of nothing prints nothing and counts nothing.
    #[test]
    fn nothing_printed_counts_nothing() {
        assert_eq!(counted(b""), Numstat::default());
    }

    /// The files that move most stand first, each with its own figures: a
    /// rename by its new path, a binary file as no lines, and only as many
    /// names as `LARGEST_FILES` allows, the rest left to the totals.
    #[test]
    fn the_files_that_move_most_stand_first_with_their_figures() {
        let printed = [
            b"1\t1\tsmall.rs\0".as_slice(),
            b"100\t20\tbig.rs\0",
            b"-\t-\tlogo.png\0",
            b"7\t0\t\0old/name.rs\0new/name.rs\0",
            b"3\t3\tmid.rs\0",
            b"2\t2\tnext.rs\0",
        ]
        .concat();
        let stat = counted(&printed);
        assert_eq!(stat.files, 6);
        let named: Vec<(&str, u64, u64)> = stat
            .largest
            .iter()
            .map(|file| (file.path.as_str(), file.added, file.removed))
            .collect();
        assert_eq!(
            named,
            [
                ("big.rs", 100, 20),
                ("new/name.rs", 7, 0),
                ("mid.rs", 3, 3),
                ("next.rs", 2, 2)
            ]
        );
    }

    /// A path that is not text is named with the replacement character, as
    /// the page shows it.
    #[test]
    fn a_path_that_is_not_text_is_named_with_the_replacement_character() {
        let stat = counted(b"5\t0\tcaf\xff.rs\0");
        assert_eq!(stat.largest[0].path, "caf\u{fffd}.rs");
    }
}
