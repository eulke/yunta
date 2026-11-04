//! What a command printed, quoted: the end of it, and where the rest is.

use std::path::PathBuf;

/// The lines evidence quotes at most: the end of what a command printed,
/// which is where a compiler and a test runner say what went wrong. More
/// buries the verdict under the output it came from; the rest is one
/// command or one path away.
const QUOTED: usize = 6;

/// Where the whole of what a command printed is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Whole {
    /// The file it is kept in: shown as a reader reads a path, and a
    /// link to where it is on a terminal that opens one.
    File { shown: String, path: PathBuf },
    /// The command that shows it.
    Command(String),
}

impl Whole {
    /// The file at `path`, shown from `cwd` and with `~` for `home`.
    pub fn file(path: PathBuf, cwd: &std::path::Path, home: Option<&std::path::Path>) -> Self {
        Whole::File {
            shown: crate::paths::shown(&path, cwd, home),
            path,
        }
    }
}

/// The end of what a command printed, and where the whole of it is.
pub struct Evidence {
    pub tail: Vec<String>,
    pub whole: Option<Whole>,
}

impl Evidence {
    /// The lines quoted: the last `QUOTED` of the tail, where a
    /// compiler or a test runner says what failed.
    pub fn quoted(&self) -> &[String] {
        let skipped = self.tail.len().saturating_sub(QUOTED);
        self.tail.get(skipped..).unwrap_or_default()
    }

    /// How many lines of the tail are left above what is quoted.
    pub fn above(&self) -> usize {
        self.tail.len() - self.quoted().len()
    }
}
