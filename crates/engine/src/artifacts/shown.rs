//! What an escalation shows, read out of the run for the person deciding.

use std::path::Path;

use yunta_core::events::findings::RunFindings;
use yunta_core::events::{AcceptedDeparture, ArtifactId, Shown, TaskLedger};

use yunta_core::{ArtifactKind, TasksFile};
use yunta_replay::RunState;

use super::store::{view_path, ObjectStore};
use crate::human_interaction::{ShownContent, ShownDocument};

/// Where the document an escalation shows sits in the run's directory:
/// the view of its canonical bytes, as the engine reads it.
pub fn view_of(shown: &Shown) -> std::path::PathBuf {
    view_path(shown.producer.as_ref(), &shown.artifact.view_name())
}

/// The documents `shows` names, from the run rooted at `run_dir`: the
/// exact bytes each hash names, a tasks document read into the plan as
/// the run `state` will judge it — with the spec it is shown beside, the
/// suite the run measured, the departures a person accepted and what
/// they `asked` the run for — a findings document into its findings.
///
/// Every surface that puts a decision to a person reads them here — the
/// prompt a run asks on and `resolve-gate` alike — so a decision is never
/// offered without what it is about.
pub async fn documents(
    run_dir: &Path,
    shows: &[Shown],
    state: &RunState,
    asked: Option<&str>,
) -> Result<Vec<ShownDocument>, crate::run::RunError> {
    Ok(held_documents(run_dir, shows, state, asked).await?)
}

async fn held_documents(
    run_dir: &Path,
    shows: &[Shown],
    state: &RunState,
    asked: Option<&str>,
) -> Result<Vec<ShownDocument>, super::HeldError> {
    let store = ObjectStore::at(run_dir);
    let answered = answered(&store, state).await?;
    let mut documents = Vec::with_capacity(shows.len());
    for shown in shows {
        let bytes = store.get(&shown.content_hash).await?;
        // Where the person deciding opens it: in full, since they read it
        // from wherever they stand and not from the run's directory.
        let path = run_dir.join(view_of(shown));
        let content = content(shown, (&bytes, &path), state, (&answered, asked))?;
        documents.push(ShownDocument {
            shown: shown.clone(),
            path,
            content,
        });
    }
    Ok(beside_its_plan(documents))
}

/// What `bytes`, the document `shown` names read at `path`, says: a plan
/// as the run in `state` judges it, with what a person `answered` and what
/// they `asked` for; a spec, a findings document, the run's findings, or
/// its text.
fn content(
    shown: &Shown,
    (bytes, path): (&[u8], &Path),
    state: &RunState,
    (answered, asked): (&[yunta_core::shown::Answered], Option<&str>),
) -> Result<ShownContent, super::HeldError> {
    Ok(match &shown.artifact {
        _ if crate::run::gate_findings::shows_view(shown) => {
            match yunta_core::yaml::parse_bytes::<RunFindings>(bytes) {
                Ok(findings) => ShownContent::RunFindings(findings),
                Err(_) => ShownContent::Text(String::from_utf8_lossy(bytes).into_owned()),
            }
        }
        ArtifactId::Interpreted {
            kind: ArtifactKind::Tasks,
        } => {
            let mut review = plan_reviewed(shown, bytes, path, state)?;
            review.answered = answered.to_vec();
            review.asked = asked.map(str::to_string);
            ShownContent::Tasks(Box::new(review))
        }
        ArtifactId::Interpreted {
            kind: ArtifactKind::Spec,
        } => ShownContent::Spec(yunta_core::shape::read::<yunta_core::SpecFile>(
            bytes,
            path.display().to_string(),
        )?),
        ArtifactId::Interpreted {
            kind: ArtifactKind::Findings,
        } => ShownContent::Findings(yunta_core::shape::read::<yunta_core::HeldFindings>(
            bytes,
            path.display().to_string(),
        )?),
        _ => ShownContent::Text(String::from_utf8_lossy(bytes).into_owned()),
    })
}

/// The documents `node` produced that a person reads whole — a plan, a
/// spec, a review's findings — a plan judged with the spec the run holds,
/// whichever node wrote it, beside what the person `asked` for.
pub async fn produced(
    run_dir: &Path,
    node: &yunta_core::NodeId,
    state: &RunState,
    asked: Option<&str>,
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
    Ok(held_documents(run_dir, &shows, state, asked).await?)
}

/// The plan `bytes` hold, as the run in `state` will judge it: with the
/// departures a person accepted, the suite the run measured, how its
/// planner got it accepted — a plan refused fifteen times reads
/// differently — and what the tree it was registered against held of its
/// shapes, as the log states it for these bytes.
fn plan_reviewed(
    shown: &Shown,
    bytes: &[u8],
    path: &Path,
    state: &RunState,
) -> Result<yunta_core::shown::PlanReview, super::HeldError> {
    let plan = yunta_core::shape::read::<TasksFile>(bytes, path.display().to_string())?;
    let departed = departed(&plan, &state.tasks);
    let held = crate::tasks::held_of(&state.run);
    let mut review = crate::tasks::plan_review(plan, None, &held, departed);
    review.handed_over = shown
        .producer
        .as_ref()
        .and_then(|node| state.artifacts.handed_over(node, ArtifactKind::Tasks))
        .cloned();
    review.shapes_read = state.tasks.shapes_read(&shown.content_hash).cloned();
    Ok(review)
}

/// Each question the run asked that a person answered, from the latest
/// questions and answers it holds.
async fn answered(
    store: &ObjectStore<'_>,
    state: &RunState,
) -> Result<Vec<yunta_core::shown::Answered>, super::HeldError> {
    let (Some(questions), Some(answers)) = (
        held::<yunta_core::QuestionsFile>(store, state).await?,
        held::<yunta_core::AnswersFile>(store, state).await?,
    ) else {
        return Ok(Vec::new());
    };
    Ok(answers
        .answers
        .into_iter()
        .filter_map(|answer| {
            let asked = questions
                .questions
                .iter()
                .find(|question| question.id == answer.id)?;
            Some(yunta_core::shown::Answered {
                question: asked.text.clone(),
                id: answer.id,
                answer: answer.value,
            })
        })
        .collect())
}

/// The latest document of its kind the run holds, read from its bytes.
async fn held<T: yunta_core::shape::Document>(
    store: &ObjectStore<'_>,
    state: &RunState,
) -> Result<Option<T>, super::HeldError> {
    let kind = ArtifactId::Interpreted { kind: T::KIND };
    let Some(held) = state.artifacts.latest(&kind, None) else {
        return Ok(None);
    };
    let bytes = store.get(&held.content_hash).await?;
    Ok(Some(yunta_core::shape::read::<T>(
        &bytes,
        held.artifact.view_name(),
    )?))
}

/// `documents` with a spec shown beside a plan read on the plan's tasks:
/// its tests are what judges them, and a second document after the plan
/// would put each test a screen away from the task it holds. The plan is
/// judged again in place, so all it was read with stands beside the
/// spec's judgement.
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
    let spec_document = documents.remove(at);
    let ShownContent::Spec(spec) = spec_document.content else {
        return documents;
    };
    for document in &mut documents {
        let planner = document.shown.producer.clone();
        if let ShownContent::Tasks(review) = &mut document.content {
            let judged = crate::tasks::plan_review(
                review.plan.clone(),
                Some(spec.clone()),
                &review.held,
                review.departed.clone(),
            );
            review.spec = judged.spec;
            review.tasks = judged.tasks;
            review.spec_node = spec_document.shown.producer.clone().or(planner);
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

#[cfg(test)]
mod tests {
    use super::*;
    use yunta_core::events::artifacts::HandedOver;

    /// A document `producer` produced, holding `content`.
    fn shown(producer: Option<&str>, kind: ArtifactKind, content: ShownContent) -> ShownDocument {
        ShownDocument {
            shown: Shown {
                producer: producer.map(Into::into),
                artifact: ArtifactId::Interpreted { kind },
                content_hash: yunta_core::ContentHash::sha256(b"bytes"),
            },
            path: std::path::PathBuf::from("artifacts/plan.yaml"),
            content,
        }
    }

    /// The spec beside a plan is read on its page: the node that wrote it,
    /// or the plan's, whose page shows the spec when no node of the run
    /// wrote it.
    #[test]
    fn a_plan_shown_beside_its_spec_names_the_page_that_shows_the_spec_whole() {
        let spec_by = |node: Option<&str>| {
            let plan = crate::tasks::plan_review(TasksFile::of(Vec::new()), None, &[], Vec::new());
            let documents = beside_its_plan(vec![
                shown(
                    Some("planner"),
                    ArtifactKind::Tasks,
                    ShownContent::Tasks(Box::new(plan)),
                ),
                shown(
                    node,
                    ArtifactKind::Spec,
                    ShownContent::Spec(yunta_core::SpecFile { specs: Vec::new() }),
                ),
            ]);
            match &documents[0].content {
                ShownContent::Tasks(review) => review.spec_node.as_ref().map(ToString::to_string),
                other => panic!("the plan comes first: {other:?}"),
            }
        };
        assert_eq!(spec_by(Some("specify")).as_deref(), Some("specify"));
        assert_eq!(spec_by(None).as_deref(), Some("planner"));
    }

    /// A plan judged beside its spec keeps everything it was read with: what
    /// the tree held of its shapes, how its planner handed it over, and
    /// what the person asked and answered.
    #[test]
    fn a_plan_judged_beside_its_spec_keeps_what_was_read_beside_it() {
        use yunta_core::events::{ShapeRead, ShapesReadPayload};
        let read = ShapesReadPayload {
            plan: yunta_core::ContentHash::sha256(b"bytes"),
            at: yunta_core::CommitSha::from("0123456789abcdef0123456789abcdef01234567"),
            shapes: vec![ShapeRead {
                name: "build_it".to_string(),
                owner: "build".into(),
                file: "src/a.rs".to_string(),
                in_tree: yunta_core::events::InTree::New,
            }],
        };
        let answered = vec![yunta_core::shown::Answered {
            id: "q1".into(),
            question: "Which depth?".to_string(),
            answer: "two".to_string(),
        }];
        let mut plan = crate::tasks::plan_review(TasksFile::of(Vec::new()), None, &[], Vec::new());
        plan.shapes_read = Some(read.clone());
        plan.handed_over = Some(HandedOver::default());
        plan.asked = Some("build it deeper".to_string());
        plan.answered = answered.clone();
        let spec = yunta_core::SpecFile { specs: Vec::new() };

        let documents = beside_its_plan(vec![
            shown(
                Some("planner"),
                ArtifactKind::Tasks,
                ShownContent::Tasks(Box::new(plan)),
            ),
            shown(
                Some("specify"),
                ArtifactKind::Spec,
                ShownContent::Spec(spec.clone()),
            ),
        ]);

        let [document] = &documents[..] else {
            panic!("the spec is read on the plan: {documents:?}");
        };
        let ShownContent::Tasks(review) = &document.content else {
            panic!("the plan stays a plan: {document:?}");
        };
        assert_eq!(review.shapes_read, Some(read));
        assert_eq!(review.handed_over, Some(HandedOver::default()));
        assert_eq!(review.asked.as_deref(), Some("build it deeper"));
        assert_eq!(review.answered, answered);
        assert_eq!(review.spec, Some(spec));
    }
}
