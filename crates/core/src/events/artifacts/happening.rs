//! What an artifact event says happened, read as a person reads it.

use std::path::PathBuf;

use crate::events::{ArtifactEvent, ArtifactId, SubmissionOutcome};
use crate::ArtifactKind;

/// One thing that happened to a document of the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happening {
    /// A document a session offered as a whole, and whether the engine
    /// took it.
    Submitted {
        kind: ArtifactKind,
        name: String,
        taken: bool,
    },
    Accepted(ArtifactId),
    Written {
        path: PathBuf,
    },
}

impl From<&ArtifactEvent> for Happening {
    fn from(event: &ArtifactEvent) -> Self {
        match event {
            ArtifactEvent::Submitted(p) => Happening::Submitted {
                kind: p.artifact_kind,
                name: p.name.clone(),
                taken: matches!(p.outcome, SubmissionOutcome::Accepted { .. }),
            },
            ArtifactEvent::Accepted(p) => Happening::Accepted(p.artifact.clone()),
            ArtifactEvent::Written(p) => Happening::Written {
                path: p.path.clone(),
            },
        }
    }
}
