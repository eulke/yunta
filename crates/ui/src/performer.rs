//! See [`crate`]. What carries out the work the application asks for, through
//! `yunta-api`, against the workspace it was made with: each operation under a stop of
//! its own, what it says and what it comes to handed back as inputs.
//!
//! A surface makes one performer and hands it every request the state asks for
//! ([`Performer::take`]): work to do starts under its ticket, work let go is stopped,
//! and what only a surface can do, showing a page or leaving, comes back to it. The
//! performer reads nothing of its process: everything comes from the workspace, so a
//! test makes it as it makes a workspace.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use surco::doc::Depth;
use tokio::sync::mpsc::UnboundedSender;
use yunta_api::{Error, Project, Remark, Remarks, RunRequest, Stop, Workspace};
use yunta_core::events::Channel;
use yunta_core::port::QuestionsReply;
use yunta_core::{Answer, NodeId, RunId};

use crate::{Input, News, Operation, Outcome, Request, Ticket};

/// Carries out an [`Operation`] through `yunta-api`, against the workspace it was made
/// with, and hands back what it says on the way and what it comes to as inputs.
pub struct Performer {
    workspace: Workspace,
    /// The stop of each piece of work under way, by its ticket.
    stops: Arc<Mutex<BTreeMap<Ticket, Stop>>>,
}

impl Performer {
    /// A performer working through `workspace`.
    pub fn new(workspace: Workspace) -> Self {
        Performer {
            workspace,
            stops: Arc::default(),
        }
    }

    /// Carries out `operation` under `ticket`, under a stop of its own: what it says
    /// and what lasting work finds go to `inputs` as they come, and what it came to
    /// last.
    pub async fn perform(
        &self,
        ticket: Ticket,
        operation: Operation,
        inputs: UnboundedSender<Input>,
    ) {
        let stop = Stop::new();
        self.held().insert(ticket, stop.clone());
        let remarks = |remark: Remark| {
            // best-effort: a state that went away no longer reads what work says.
            let _ = inputs.send(Input::Remark(ticket, remark));
        };
        let found = |news: News| {
            // best-effort: a state that went away no longer reads what work finds.
            let _ = inputs.send(Input::News(ticket, news));
        };
        let outcome = match self.carried_out(operation, &remarks, &found, &stop).await {
            Ok(outcome) => outcome,
            Err(Error::Refused(refused)) => Outcome::Refused(refused),
            Err(Error::Failed(failure)) => Outcome::Failed(failure.to_string()),
            Err(Error::Stopped) => Outcome::Stopped,
        };
        self.held().remove(&ticket);
        // best-effort: a state that went away no longer reads what work came to.
        let _ = inputs.send(Input::Outcome(ticket, outcome));
    }

    /// Takes `request`: work to do starts under its ticket, in a task of its own, and
    /// hands what it comes to to `inputs`; work let go is stopped. What only a surface
    /// can do, showing a page or leaving, comes back.
    pub fn take(
        self: &Arc<Self>,
        request: Request,
        inputs: &UnboundedSender<Input>,
    ) -> Option<Request> {
        match request {
            Request::Perform(ticket, operation) => {
                let (performer, inputs) = (Arc::clone(self), inputs.clone());
                tokio::spawn(async move { performer.perform(ticket, operation, inputs).await });
                None
            }
            Request::LetGo(ticket) => {
                self.let_go(ticket);
                None
            }
            Request::ShowPage(_) | Request::Leave => Some(request),
        }
    }

    /// Stops the work under `ticket`, if it is under way, and drops what is held for it.
    pub fn let_go(&self, ticket: Ticket) {
        if let Some(stop) = self.held().remove(&ticket) {
            stop.stop();
        }
    }

    /// What `operation` comes to, through the API, what it says on the way said
    /// through `remarks` and what lasting work finds handed to `found`.
    async fn carried_out(
        &self,
        operation: Operation,
        remarks: &dyn Remarks,
        found: &(dyn Fn(News) + Sync),
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let project = self.workspace.project()?;
        match operation {
            Operation::ReadRepository => Ok(Outcome::Repository(project.repository(stop).await?)),
            Operation::ReadCatalog => Ok(Outcome::Catalog(Box::new(
                project.workspace.catalog().await,
            ))),
            Operation::Field { reach } => Ok(Outcome::Field {
                reading: project.field(reach, stop).await?,
                storage: project.storage_path.clone(),
            }),
            Operation::Follow { run } => followed(&project, &run, found, stop).await,
            Operation::ReadPage { run } => {
                let page = project.run(&run).await?;
                Ok(Outcome::Page(Box::new(
                    page.page(Depth::Glance, remarks).await?,
                )))
            }
            Operation::ReadNodePage { run, node } => {
                let page = project.run(&run).await?.node_page(&node, remarks).await?;
                Ok(Outcome::NodePage(Box::new(page)))
            }
            Operation::ReadQuestions { run } => {
                let waiting = project.run(&run).await?.waiting_questions().await?;
                Ok(Outcome::Questions(Box::new(waiting)))
            }
            Operation::ReadDecision { run } => {
                let waiting = project.run(&run).await?.waiting_decision().await?;
                Ok(Outcome::Decision(Box::new(waiting)))
            }
            Operation::Decide { run, option, words } => {
                project.run(&run).await?.decide(option, None, words).await?;
                Ok(Outcome::Decided { run })
            }
            Operation::Answer { run, node, answers } => {
                answered(&project, run, &node, answers).await
            }
            Operation::Resume { run } => resumed(&project, run).await,
            Operation::Cancel { run } => {
                let state = project.run(&run).await?.cancel(remarks, stop).await?;
                Ok(Outcome::Cancelled { run, state })
            }
            Operation::Close { runs } => closed(&project, runs, remarks, stop).await,
            Operation::Start { request } => started(&project, request, remarks, stop).await,
        }
    }

    /// The stops held, whatever a panicking holder left them as.
    fn held(&self) -> std::sync::MutexGuard<'_, BTreeMap<Ticket, Stop>> {
        self.stops.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `run` followed until it no longer moves: the run as its following reads it first,
/// then what each look at it finds, handed to `found` as it comes.
async fn followed(
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

/// `answers` to the round `node` asked recorded on `run`, and the run handed on to a
/// detached resume.
async fn answered(
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
async fn started(
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

/// `run` handed back to a detached resume, refused where another engine drives it.
async fn resumed(project: &Project, run: RunId) -> Result<Outcome, Error> {
    let open = project.run(&run).await?;
    open.refuse_if_driven().await?;
    open.resume_detached().await?;
    Ok(Outcome::Resumed { run })
}

/// Each of `runs` closed for good, why each refused one was refused kept beside the
/// ones closed; any other failure ends the closing there.
async fn closed(
    project: &Project,
    runs: Vec<RunId>,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Outcome, Error> {
    let (mut closed, mut refused) = (Vec::new(), Vec::new());
    for run in runs {
        let closing = match project.run(&run).await {
            Ok(open) => open.close(None, remarks, stop).await.map(drop),
            Err(error) => Err(error),
        };
        match closing {
            Ok(()) => closed.push(run),
            Err(Error::Refused(why)) => refused.push(why),
            Err(other) => return Err(other),
        }
    }
    Ok(Outcome::Closed { closed, refused })
}
