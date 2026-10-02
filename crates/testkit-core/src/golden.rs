//! A surface's whole output, compared with the output written down for
//! it.
//!
//! A golden is a file holding exactly what one surface printed for one
//! case in one [`Environment`]. A test renders the case and compares the
//! two whole: an assertion that looks for a phrase passes while the rest
//! of the screen around it changes, and the screen is what a person
//! reads. A difference fails naming the first line that differs, with the
//! lines above it, and every escape is written as `␛` — in the file and
//! in the failure — so a change of color reads as a change of text.
//!
//! `YUNTA_BLESS=1` writes what was rendered instead of comparing, for a
//! change that means to change what a surface prints. The new files are
//! reviewed in the diff like any other change.

use std::path::{Path, PathBuf};

/// What a terminal a golden is rendered for looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Environment {
    /// The name its goldens carry: `<case>.<name>.txt`.
    pub name: &'static str,
    /// Whether the terminal draws Unicode marks, or only ASCII.
    pub unicode: bool,
    /// Whether the stream gets color.
    pub color: bool,
    /// The cells a line may take.
    pub width: usize,
}

/// The environments every surface is rendered in: a wide terminal that
/// draws Unicode and color, a plain one at the width a pasted line has,
/// and the narrowest a surface is laid out at.
pub const ENVIRONMENTS: [Environment; 3] = [
    Environment {
        name: "unicode-color-100",
        unicode: true,
        color: true,
        width: 100,
    },
    Environment {
        name: "ascii-80",
        unicode: false,
        color: false,
        width: 80,
    },
    Environment {
        name: "unicode-60",
        unicode: true,
        color: false,
        width: 60,
    },
];

impl Environment {
    /// The golden `case` has in this environment, under `dir`.
    pub fn golden(&self, dir: &Path, case: &str) -> PathBuf {
        dir.join(format!("{case}.{}.txt", self.name))
    }
}

/// Fails unless `actual` is what the golden at `path` holds — or, with
/// `YUNTA_BLESS=1`, writes `actual` there.
pub fn assert_golden(path: &Path, actual: &str) {
    let bless = std::env::var("YUNTA_BLESS").is_ok_and(|value| value == "1");
    if let Err(difference) = check(path, actual, bless) {
        panic!("{difference}");
    }
}

/// `text` with every escape it carries written as `␛`.
pub fn visible(text: &str) -> String {
    text.replace('\x1b', "␛")
}

/// Compares `actual` with the golden at `path`, or writes it there when
/// `bless` says to. The failure is the sentence a test fails with.
fn check(path: &Path, actual: &str, bless: bool) -> Result<(), String> {
    let actual = visible(actual);
    if bless {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        return std::fs::write(path, &actual).map_err(|e| format!("{}: {e}", path.display()));
    }
    let expected = std::fs::read_to_string(path).map_err(|_| {
        format!(
            "no golden at {} — `YUNTA_BLESS=1` writes it from this run",
            path.display()
        )
    })?;
    match first_difference(&expected, &actual) {
        None => Ok(()),
        Some(at) => Err(format!(
            "{} differs from what was rendered at line {}:\n{}\n\
             `YUNTA_BLESS=1` writes what was rendered, if the change is meant",
            path.display(),
            at + 1,
            around(&expected, &actual, at)
        )),
    }
}

/// The index of the first line `expected` and `actual` disagree on,
/// counting a line one of them lacks; `None` when they are equal.
fn first_difference(expected: &str, actual: &str) -> Option<usize> {
    if expected == actual {
        return None;
    }
    let (wanted, got): (Vec<&str>, Vec<&str>) =
        (expected.split('\n').collect(), actual.split('\n').collect());
    (0..wanted.len().max(got.len())).find(|at| wanted.get(*at) != got.get(*at))
}

/// The lines just above line `at`, which both texts share, then line
/// `at` as each of them has it.
fn around(expected: &str, actual: &str, at: usize) -> String {
    const CONTEXT: usize = 2;
    let wanted: Vec<&str> = expected.split('\n').collect();
    let got: Vec<&str> = actual.split('\n').collect();
    let mut out: Vec<String> = (at.saturating_sub(CONTEXT)..at)
        .filter_map(|line| wanted.get(line))
        .map(|line| format!("    {line}"))
        .collect();
    out.push(format!("  - {}", wanted.get(at).unwrap_or(&"(no line)")));
    out.push(format!("  + {}", got.get(at).unwrap_or(&"(no line)")));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_golden_that_differs_names_the_first_line_that_differs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("closing.ascii-80.txt");
        std::fs::write(&path, "run X: finished\n  progress  nodes 2/2\n  next  a\n").unwrap();

        let difference = check(
            &path,
            "run X: finished\n  progress  nodes 2/2\n  next  b\n",
            false,
        )
        .expect_err("the third line differs");
        assert!(difference.contains("at line 3"), "{difference}");
        assert!(
            difference.contains("    run X: finished")
                && difference.contains("  - ")
                && difference.contains("  next  a")
                && difference.contains("  + ")
                && difference.contains("  next  b"),
            "the lines above it, then the line each has: {difference}"
        );
    }

    #[test]
    fn bless_writes_what_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        let path = ENVIRONMENTS[0].golden(&dir.path().join("closing"), "finished");
        let rendered = "run X: \x1b[32m✓\x1b[0m finished\n";

        check(&path, rendered, true).expect("blessing writes the golden");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "run X: ␛[32m✓␛[0m finished\n",
            "an escape is written where a reviewer can read it"
        );
        assert_eq!(check(&path, rendered, false), Ok(()));
    }

    #[test]
    fn a_missing_golden_says_how_to_write_it() {
        let dir = tempfile::tempdir().unwrap();
        let missing = check(&dir.path().join("none.txt"), "x", false).unwrap_err();
        assert!(missing.contains("YUNTA_BLESS=1"), "{missing}");
    }

    #[test]
    fn a_line_one_side_lacks_is_a_difference() {
        assert_eq!(first_difference("a\nb", "a\nb\nc"), Some(2));
        assert_eq!(first_difference("a\nb", "a\nb"), None);
    }
}
