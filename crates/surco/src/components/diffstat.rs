//! What a change came to, file by file.

use crate::figures::files_moving;
use crate::glyphs::Glyphs;
use crate::text::counted;

/// What a table of a change calls its columns: the file, and what it adds and
/// takes out.
pub const COLUMNS: [&str; 3] = ["file", "added", "removed"];

/// The files a change moves most, and what the rest come to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diffstat {
    pub files: Vec<FileRow>,
    pub more: Option<MoreFiles>,
}

/// One file of a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub path: String,
    pub added: u64,
    pub removed: u64,
}

/// What the files a change does not name come to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoreFiles {
    pub files: u32,
    pub added: u64,
    pub removed: u64,
}

impl Diffstat {
    /// The lines the file that moves most moves: what every bar is drawn against.
    pub fn scale(&self) -> u64 {
        self.files
            .iter()
            .map(|file| file.added + file.removed)
            .max()
            .unwrap_or(0)
    }
}

impl MoreFiles {
    /// The rest as a sentence: `5 more files, +123 −16`.
    pub fn said(&self, glyphs: Glyphs) -> String {
        let files = counted(self.files as usize, "more file");
        files_moving(files, self.added, self.removed, glyphs)
    }
}
