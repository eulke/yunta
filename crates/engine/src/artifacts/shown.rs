//! What an escalation shows, read out of the run for the person deciding.

use std::path::Path;

use yunta_core::events::{ArtifactId, Shown};
use yunta_core::{ArtifactKind, TasksFile};

use super::store::{view_path, ObjectStore};
use crate::human_interaction::{ShownContent, ShownDocument};

/// Why a shown document could not be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ShownError {
    #[error(transparent)]
    Object(#[from] super::ObjectError),
    #[error("{0}")]
    Unreadable(#[from] yunta_core::diagnostic::Report),
}

impl From<ShownError> for crate::run::RunError {
    fn from(error: ShownError) -> Self {
        match error {
            ShownError::Object(source) => Self::Object(source),
            ShownError::Unreadable(report) => Self::UnreadableArtifact(report),
        }
    }
}

/// The file a person opens to read what an escalation shows: a plan's
/// view for them, any other document's own.
pub fn view_of(shown: &Shown) -> std::path::PathBuf {
    let name = match &shown.artifact {
        ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks,
        } => crate::tasks::view::VIEW_NAME.to_string(),
        other => other.view_name(),
    };
    view_path(shown.producer.as_ref(), &name)
}

/// The documents `shows` names, from the run rooted at `run_dir`: the
/// exact bytes each hash names, a tasks document read into its tasks.
pub(crate) async fn documents(
    run_dir: &Path,
    shows: &[Shown],
) -> Result<Vec<ShownDocument>, ShownError> {
    let store = ObjectStore::at(run_dir);
    let mut documents = Vec::with_capacity(shows.len());
    for shown in shows {
        let bytes = store.get(&shown.content_hash).await?;
        // Where the person deciding opens it: in full, since they read it
        // from wherever they stand and not from the run's directory.
        let path = run_dir.join(view_of(shown));
        let content = match &shown.artifact {
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            } => ShownContent::Tasks(yunta_core::shape::read::<TasksFile>(
                &bytes,
                path.display().to_string(),
            )?),
            _ => ShownContent::Text(String::from_utf8_lossy(&bytes).into_owned()),
        };
        documents.push(ShownDocument {
            shown: shown.clone(),
            path,
            content,
        });
    }
    Ok(documents)
}
