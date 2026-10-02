//! What an escalation shows, read out of the run for the person deciding.

use std::path::Path;

use yunta_core::events::findings::RunFindings;
use yunta_core::events::{AcceptedDeparture, ArtifactId, Shown, TaskLedger};

use crate::replay::RunState;
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
/// exact bytes each hash names, a tasks document read into the plan as
/// the run `state` will judge it — with the spec it is shown beside, the
/// suite the run measured and the departures a person accepted — a
/// findings document into its findings.
///
/// Every surface that puts a decision to a person reads them here — the
/// prompt a run asks on and `resolve-gate` alike — so a decision is never
/// offered without what it is about.
pub async fn documents(
    run_dir: &Path,
    shows: &[Shown],
    state: &RunState,
) -> Result<Vec<ShownDocument>, crate::run::RunError> {
    Ok(held_documents(run_dir, shows, state).await?)
}

async fn held_documents(
    run_dir: &Path,
    shows: &[Shown],
    state: &RunState,
) -> Result<Vec<ShownDocument>, super::HeldError> {
    let store = ObjectStore::at(run_dir);
    let mut documents = Vec::with_capacity(shows.len());
    for shown in shows {
        let bytes = store.get(&shown.content_hash).await?;
        // Where the person deciding opens it: in full, since they read it
        // from wherever they stand and not from the run's directory.
        let path = run_dir.join(view_of(shown));
        let content = match &shown.artifact {
            _ if crate::run::gate_findings::shows_view(shown) => {
                match yunta_core::yaml::parse_bytes::<RunFindings>(&bytes) {
                    Ok(findings) => ShownContent::RunFindings(findings),
                    Err(_) => ShownContent::Text(String::from_utf8_lossy(&bytes).into_owned()),
                }
            }
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            } => {
                let plan =
                    yunta_core::shape::read::<TasksFile>(&bytes, path.display().to_string())?;
                let departed = departed(&plan, &state.tasks);
                let suite = crate::tasks::suite_of(state.run.baseline());
                ShownContent::Tasks(Box::new(crate::tasks::plan_review(
                    plan, None, suite, departed,
                )))
            }
            ArtifactId::Interpreted {
                kind: ArtifactKind::Spec,
            } => ShownContent::Spec(yunta_core::shape::read::<yunta_core::SpecFile>(
                &bytes,
                path.display().to_string(),
            )?),
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
    Ok(beside_its_plan(documents))
}

/// The documents `node` produced that a person reads whole — a plan, a
/// spec, a review's findings — a plan judged with the spec the run holds,
/// whichever node wrote it.
pub async fn produced(
    run_dir: &Path,
    node: &yunta_core::NodeId,
    state: &RunState,
) -> Result<Vec<ShownDocument>, crate::run::RunError> {
    let read = |artifact: &ArtifactId| {
        matches!(
            artifact,
            ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks | ArtifactKind::Spec | ArtifactKind::Findings
            }
        )
    };
    let shown = |held: &yunta_core::events::artifacts::ledger::ArtifactRef| Shown {
        producer: held.producer.clone(),
        artifact: held.artifact.clone(),
        content_hash: held.content_hash.clone(),
    };
    let mut shows: Vec<Shown> = state
        .artifacts
        .by_producer(node)
        .filter(|held| read(&held.artifact))
        .map(shown)
        .collect();
    let spec = ArtifactId::Interpreted {
        kind: ArtifactKind::Spec,
    };
    let plans = shows.iter().any(|shown| {
        shown.artifact
            == ArtifactId::Interpreted {
                kind: ArtifactKind::Tasks,
            }
    });
    if plans && !shows.iter().any(|shown| shown.artifact == spec) {
        shows.extend(state.artifacts.latest(&spec, None).map(shown));
    }
    Ok(held_documents(run_dir, &shows, state).await?)
}

/// `documents` with a spec shown beside a plan read on the plan's tasks:
/// its tests are what judges them, and a second document after the plan
/// would put each test a screen away from the task it holds.
fn beside_its_plan(mut documents: Vec<ShownDocument>) -> Vec<ShownDocument> {
    let has_plan = documents
        .iter()
        .any(|document| matches!(document.content, ShownContent::Tasks(_)));
    let spec_at = documents
        .iter()
        .position(|document| matches!(document.content, ShownContent::Spec(_)));
    let (true, Some(at)) = (has_plan, spec_at) else {
        return documents;
    };
    let ShownContent::Spec(spec) = documents.remove(at).content else {
        return documents;
    };
    for document in &mut documents {
        if let ShownContent::Tasks(review) = &mut document.content {
            **review = crate::tasks::plan_review(
                review.plan.clone(),
                Some(spec.clone()),
                review.suite.as_deref(),
                review.departed.clone(),
            );
        }
    }
    documents
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
