//! A run a field can name but cannot derive, and what stops it from
//! reading back: its event log, the directory that should hold it, or the
//! manifest it froze. The page says each; a document carries the same
//! sentence.

use std::path::PathBuf;

use yunta_core::RunId;

/// A run a field can name but not derive: its log or the manifest it froze
/// does not read back. It is listed as itself, never dropped — a run missing
/// from a listing is a run nobody goes looking for.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnreadableRun {
    pub run_id: RunId,
    /// What stops it from reading back, which a document carries as the
    /// sentence the page says.
    #[serde(serialize_with = "said")]
    pub problem: RunUnread,
}

impl UnreadableRun {
    pub fn new(run_id: &RunId, problem: RunUnread) -> Self {
        UnreadableRun {
            run_id: run_id.clone(),
            problem,
        }
    }
}

/// What stops a run from reading back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunUnread {
    /// Its event log does not read back, for this.
    EventLog(String),
    /// No directory under the runs root holds it.
    NoDirectory { runs_root: PathBuf },
    /// Its manifest is missing, or does not parse, at this path.
    Manifest { path: PathBuf },
}

impl std::fmt::Display for RunUnread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunUnread::EventLog(why) => write!(f, "its event log does not read back: {why}"),
            RunUnread::NoDirectory { runs_root } => write!(
                f,
                "manifest missing or unreadable under {}",
                runs_root.display()
            ),
            RunUnread::Manifest { path } => {
                write!(f, "manifest missing or unreadable at {}", path.display())
            }
        }
    }
}

/// `problem` as the sentence the page says.
fn said<S: serde::Serializer>(problem: &RunUnread, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(problem)
}
