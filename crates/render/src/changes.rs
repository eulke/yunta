//! What a run changed, as git counted it from its base to its newest tree:
//! the one wording every surface says it in.

use surco::components::{Diffstat, FileRow, MoreFiles};
use surco::figures::files_moving;
use surco::glyphs::Glyphs;
use surco::text::{counted, one_line};
use yunta_core::events::ChangesCountedPayload;

/// The files a change touches and the lines it adds and takes out, as a
/// diffstat reads: `9 files, +412 −87`. A sign whose count is zero is left
/// out, so a change of binary files alone reads `2 files`. Nothing for a
/// change that touched no file: a count of zero says nothing.
pub fn said(files: u32, added: u64, removed: u64, glyphs: Glyphs) -> Option<String> {
    (files > 0).then(|| files_moving(counted(files as usize, "file"), added, removed, glyphs))
}

/// [`said`] of a counted change.
pub fn of(change: &ChangesCountedPayload, glyphs: Glyphs) -> Option<String> {
    said(change.files, change.added, change.removed, glyphs)
}

/// The files `change` names, and what the rest come to, each path on the one line
/// a row has for it. Nothing for a change that names none: a log older than the
/// files, or a change of no file.
pub fn by_file(change: &ChangesCountedPayload) -> Option<Diffstat> {
    if change.largest.is_empty() {
        return None;
    }
    let files: Vec<FileRow> = change
        .largest
        .iter()
        .map(|file| FileRow {
            path: one_line(&file.path),
            added: file.added,
            removed: file.removed,
        })
        .collect();
    let named = u32::try_from(files.len()).unwrap_or(u32::MAX);
    let added: u64 = files.iter().map(|file| file.added).sum();
    let removed: u64 = files.iter().map(|file| file.removed).sum();
    let more = (change.files > named).then(|| MoreFiles {
        files: change.files - named,
        added: change.added.saturating_sub(added),
        removed: change.removed.saturating_sub(removed),
    });
    Some(Diffstat { files, more })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file's name may hold a tab or a line break; the row that names it has
    /// one line, and its figures stand in their column.
    #[test]
    fn a_path_is_named_on_one_line() {
        let change = ChangesCountedPayload {
            tree: "deadbeef".into(),
            files: 1,
            added: 1,
            removed: 0,
            largest: vec![yunta_core::events::FileChange {
                path: "a\tb\nc.rs".to_string(),
                added: 1,
                removed: 0,
            }],
        };
        let stat = by_file(&change).expect("a file it names");
        assert_eq!(stat.files[0].path, "a b c.rs");
    }

    /// A change reads as its files and its signed lines, a zero left out,
    /// and a change of no file says nothing.
    #[test]
    fn a_change_reads_as_its_files_and_its_signed_lines() {
        assert_eq!(
            said(9, 412, 87, Glyphs::Unicode).as_deref(),
            Some("9 files, +412 −87")
        );
        assert_eq!(
            said(9, 412, 87, Glyphs::Ascii).as_deref(),
            Some("9 files, +412 -87")
        );
        assert_eq!(
            said(1, 3, 0, Glyphs::Unicode).as_deref(),
            Some("1 file, +3")
        );
        assert_eq!(said(2, 0, 0, Glyphs::Unicode).as_deref(), Some("2 files"));
        assert_eq!(said(0, 0, 0, Glyphs::Unicode), None);
    }

    #[test]
    fn a_change_names_its_largest_files_and_sums_the_rest() {
        let change = ChangesCountedPayload {
            tree: "deadbeef".into(),
            files: 9,
            added: 412,
            removed: 87,
            largest: vec![yunta_core::events::FileChange {
                path: "a.rs".to_string(),
                added: 100,
                removed: 7,
            }],
        };
        let stat = by_file(&change).expect("a file it names");
        assert_eq!(stat.files.len(), 1);
        let more = stat.more.expect("the rest");
        assert_eq!((more.files, more.added, more.removed), (8, 312, 80));
        let all = ChangesCountedPayload {
            files: 1,
            ..change.clone()
        };
        assert!(by_file(&all).and_then(|stat| stat.more).is_none());
        let none = ChangesCountedPayload {
            largest: Vec::new(),
            ..change
        };
        assert!(by_file(&none).is_none());
    }
}
