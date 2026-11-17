//! See [`super`]. The work a run's stages ask for: its readings and the acts that change
//! it, each through the API's open run.

use surco::doc::Depth;
use yunta_api::{Error, Project, Remarks, RunRequest, Stop};
use yunta_core::events::Channel;
use yunta_core::port::QuestionsReply;
use yunta_core::{Answer, NodeId, OptionId, RunId};

use crate::Outcome;

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

/// `answers` to the round `node` asked recorded on `run`, under the `channel` a person
/// reached the performer through, and the run handed on to a detached resume.
pub(super) async fn answered(
    project: &Project,
    run: RunId,
    node: &NodeId,
    answers: Vec<Answer>,
    channel: Channel,
) -> Result<Outcome, Error> {
    let reply = QuestionsReply {
        answers,
        channel,
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
