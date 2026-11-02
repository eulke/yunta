//! What a run changed, as git counted it from its base to its newest tree:
//! the one wording every surface says it in.

use surco::glyphs::Glyphs;
use surco::text::counted;
use yunta_core::events::ChangesCountedPayload;

/// The files a change touches and the lines it adds and takes out, as a
/// diffstat reads: `9 files, +412 −87`. A sign whose count is zero is left
/// out, so a change of binary files alone reads `2 files`. Nothing for a
/// change that touched no file: a count of zero says nothing.
pub fn said(files: u32, added: u64, removed: u64, glyphs: Glyphs) -> Option<String> {
    if files == 0 {
        return None;
    }
    let [plus, minus] = glyphs.changed();
    let lines: Vec<String> = [(plus, added), (minus, removed)]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(sign, count)| format!("{sign}{count}"))
        .collect();
    let files = counted(files as usize, "file");
    Some(match lines.is_empty() {
        true => files,
        false => format!("{files}, {}", lines.join(" ")),
    })
}

/// [`said`] of a counted change.
pub fn of(change: &ChangesCountedPayload, glyphs: Glyphs) -> Option<String> {
    said(change.files, change.added, change.removed, glyphs)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
