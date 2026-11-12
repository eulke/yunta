//! See [`super`]. The work a run's stages ask for: its readings, what it spent and
//! its graph among them, its following, and the acts that change it, each through
//! the API's open run.

use surco::doc::Depth;
use yunta_api::{Error, FieldReach, Project, Remarks, RunRequest, Stop};
use yunta_core::events::Channel;
use yunta_core::port::QuestionsReply;
use yunta_core::{Answer, NodeId, OptionId, RunId};
use yunta_render::page::Graph;

use crate::{News, Outcome};

/// The runs as far as `reach` reaches, and where the project keeps them.
pub(super) async fn field_read(
    project: &Project,
    reach: FieldReach,
    stop: &Stop,
) -> Result<Outcome, Error> {
    Ok(Outcome::Field {
        reading: project.field(reach, stop).await?,
        storage: project.storage_path.clone(),
    })
}

/// `run`'s page, what reading it says said through `remarks`.
pub(super) async fn page_read(
    project: &Project,
    run: &RunId,
    remarks: &dyn Remarks,
) -> Result<Outcome, Error> {
    let page = project.run(run).await?.page(Depth::Glance, remarks).await?;
    Ok(Outcome::Page(Box::new(page)))
}

/// The page of `run`'s node `node`, what reading it says said through `remarks`.
pub(super) async fn node_page_read(
    project: &Project,
    run: &RunId,
    node: &NodeId,
    remarks: &dyn Remarks,
) -> Result<Outcome, Error> {
    let page = project.run(run).await?.node_page(node, remarks).await?;
    Ok(Outcome::NodePage(Box::new(page)))
}

/// The round of questions `run` is parked on.
pub(super) async fn questions_read(project: &Project, run: &RunId) -> Result<Outcome, Error> {
    let waiting = project.run(run).await?.waiting_questions().await?;
    Ok(Outcome::Questions(Box::new(waiting)))
}

/// The decision `run` waits on, with the documents it shows.
pub(super) async fn decision_read(project: &Project, run: &RunId) -> Result<Outcome, Error> {
    let waiting = project.run(run).await?.waiting_decision().await?;
    Ok(Outcome::Decision(Box::new(waiting)))
}

/// What `run` spent, read at the project's clock's instant, as `yunta stats` reads
/// it.
pub(super) async fn stats_read(project: &Project, run: &RunId) -> Result<Outcome, Error> {
    let stats = project.run(run).await?.stats();
    Ok(Outcome::Stats(Box::new(stats)))
}

/// The workflow `run` froze, each node labelled with the state its log derives for
/// it, as `yunta graph --run` draws it.
pub(super) async fn graph_read(project: &Project, run: RunId) -> Result<Outcome, Error> {
    let open = project.run(&run).await?;
    let graph = Graph {
        workflow: open.manifest.doc.workflow.clone(),
        run: Some((run, open.frame())),
    };
    Ok(Outcome::Graph(Box::new(graph)))
}

/// `run` followed until it no longer moves: the run as its following reads it first,
/// then what each look at it finds, handed to `found` as it comes.
pub(super) async fn followed(
    project: &Project,
    run: &RunId,
    found: &(dyn Fn(News) + Sync),
    stop: &Stop,
) -> Result<Outcome, Error> {
    let mut following = project.follow(run).await?;
    found(News::Following(Box::new(following.followed().clone())));
    while let Some(news) = following.next(stop).await? {
        found(News::Run(news));
    }
    Ok(Outcome::Followed)
}

/// `option` recorded on the decision `run` waits on, with the `words` it was chosen
/// with, and the run handed on to a detached resume.
pub(super) async fn decided(
    project: &Project,
    run: RunId,
    option: OptionId,
    words: Option<String>,
) -> Result<Outcome, Error> {
    project.run(&run).await?.decide(option, None, words).await?;
    Ok(Outcome::Decided { run })
}

/// `answers` to the round `node` asked recorded on `run`, and the run handed on to a
/// detached resume.
pub(super) async fn answered(
    project: &Project,
    run: RunId,
    node: &NodeId,
    answers: Vec<Answer>,
) -> Result<Outcome, Error> {
    // A person answers the application where it is drawn: on a terminal, as every
    // surface of it does today.
    let reply = QuestionsReply {
        answers,
        channel: Channel::Tty,
        responder: None,
    };
    project.run(&run).await?.answer(node, reply).await?;
    Ok(Outcome::Answered { run })
}

/// The run `request` asks for, admitted, created and started detached, as
/// `yunta run --detach` starts it, what admission says said through `remarks`.
pub(super) async fn started(
    project: &Project,
    request: RunRequest,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Outcome, Error> {
    let admission = project.admit(request, remarks, stop).await?;
    let created = admission.create(remarks, stop).await?;
    let run = created.start_detached(stop).await?;
    Ok(Outcome::Started { run })
}

/// `run` cancelled with everything under it, what cancelling says said through
/// `remarks`, and how it ended.
pub(super) async fn cancelled(
    project: &Project,
    run: RunId,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Outcome, Error> {
    let state = project.run(&run).await?.cancel(remarks, stop).await?;
    Ok(Outcome::Cancelled { run, state })
}

/// `run` handed back to a detached resume, refused where another engine drives it.
pub(super) async fn resumed(project: &Project, run: RunId) -> Result<Outcome, Error> {
    let open = project.run(&run).await?;
    open.refuse_if_driven().await?;
    open.resume_detached().await?;
    Ok(Outcome::Resumed { run })
}

/// Each of `runs` closed for good, why each refused one was refused kept beside the
/// ones closed; any other failure ends the closing there.
pub(super) async fn closed(
    project: &Project,
    runs: Vec<RunId>,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Outcome, Error> {
    let (mut closed, mut refused, mut failed) = (Vec::new(), Vec::new(), None);
    for run in runs {
        let closing = match project.run(&run).await {
            Ok(open) => open.close(None, remarks, stop).await.map(drop),
            Err(error) => Err(error),
        };
        match closing {
            Ok(()) => closed.push(run),
            Err(Error::Refused(why)) => refused.push(why),
            Err(Error::Failed(failure)) => {
                failed = Some(failure.to_string());
                break;
            }
            Err(Error::Stopped) => return Err(Error::Stopped),
        }
    }
    Ok(Outcome::Closed {
        closed,
        refused,
        failed,
    })
}
