//! What an escalation shows, read out of the run for the person deciding.

use std::path::Path;

use yunta_core::events::{AcceptedDeparture, ArtifactId, Shown, TaskLedger};
use yunta_core::{ArtifactKind, TasksFile};

use super::store::{view_path, ObjectStore};
use crate::human_interaction::{ShownContent, ShownDocument};

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
/// exact bytes each hash names, a tasks document read into its tasks and
/// shown with the departures from it `tasks` records as accepted, a
/// findings document into its findings.
pub(crate) async fn documents(
    run_dir: &Path,
    shows: &[Shown],
    tasks: &TaskLedger,
) -> Result<Vec<ShownDocument>, super::HeldError> {
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
            } => {
                let plan =
                    yunta_core::shape::read::<TasksFile>(&bytes, path.display().to_string())?;
                let departed = departed(&plan, tasks);
                ShownContent::Tasks { plan, departed }
            }
            ArtifactId::Interpreted {
                kind: ArtifactKind::Findings,
            } => ShownContent::Findings(yunta_core::shape::read::<yunta_core::FindingsFile>(
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

/// Every departure from `plan` a person accepted, task by task in the
/// plan's order, each task's in the order its sessions declared them.
fn departed(plan: &TasksFile, tasks: &TaskLedger) -> Vec<AcceptedDeparture> {
    plan.tasks
        .iter()
        .filter_map(|task| tasks.get(&task.id))
        .flat_map(|record| record.departures_accepted.iter().cloned())
        .collect()
}
