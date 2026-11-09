//! See [`super`]. A mock fixture a run's sessions are scripted by, read for a
//! run that does not exist yet. The file is parsed here too, so one that does
//! not parse refuses the run before it is created, and the refusal names the
//! file. The run's directories are unknown until the run exists, so each
//! placeholder renders to `.` meanwhile: a plain path, which leaves the
//! fixture's shape as a real run's directories leave it.

use std::path::{Path, PathBuf};

use yunta_adapters::{FixtureError, MockFixture, RunPaths};

use crate::{Error, Failure};

/// A fixture file that could not be read, or that does not parse.
#[derive(Debug, thiserror::Error)]
enum FixtureUnread {
    #[error("failed to read fixture {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The path leads the sentence, because a person running several cases
    /// needs to know which fixture broke before they need to know how.
    #[error("fixture `{}`: {source}", path.display())]
    Refused {
        path: PathBuf,
        #[source]
        source: FixtureError,
    },
}

/// The mock fixture at `path`, checked to parse, as the text a run pinned to
/// the mock freezes.
pub async fn read_fixture(path: &Path) -> Result<String, Error> {
    let text = tokio::fs::read_to_string(path).await.map_err(|source| {
        Failure::new(FixtureUnread::Read {
            path: path.to_path_buf(),
            source,
        })
    })?;
    let placeholder = Path::new(".");
    let staging = yunta_engine::run_dir::staging_root(placeholder);
    let paths = RunPaths {
        run_dir: placeholder,
        worktree: placeholder,
        staging: &staging,
    };
    MockFixture::parse(&text, &paths).map_err(|source| {
        Failure::new(FixtureUnread::Refused {
            path: path.to_path_buf(),
            source,
        })
    })?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture that is not YAML at all is refused at birth, and the refusal
    /// names the file, so the person knows which fixture to fix.
    #[tokio::test]
    async fn a_fixture_that_does_not_parse_is_refused_naming_its_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("broken.yaml");
        std::fs::write(&path, "sessions: [unclosed\n").unwrap();
        let error = read_fixture(&path)
            .await
            .expect_err("a fixture that is not YAML is refused");
        assert!(error.to_string().contains("broken.yaml"), "{error}");
    }
}
