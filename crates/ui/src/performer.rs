//! See [`crate`]. What carries out the work the application asks for, through
//! `yunta-api`, against the workspace it was made with: each operation under a stop of
//! its own, what it says and what it comes to handed back as inputs.
//!
//! A surface makes one performer and hands it every request the state asks for
//! ([`Performer::take`]): work to do starts under its ticket, work let go is stopped,
//! what to follow goes to its following, and what only a surface can do, showing a page,
//! calling the person or leaving, comes back to it. The performer reads nothing of its
//! process: everything comes from the workspace, so a test makes it as it makes a
//! workspace. Nor does it know where it is drawn: the surface that makes it hands it the
//! channel a person reaches it through, which every answer it records carries, so the
//! interface names no terminal.
//!
//! It keeps one following, made on the first request to follow and followed anew on
//! each after it, made anew where a request hands its news to other inputs, which never
//! ends by itself: dropping the performer ends it, `following`'s.
//!
//! Each family of work is carried out in a module of its own: `run`, the pull request a
//! run's gate waits on and the acts on a run; `evidence`, what a run leaves; `workflow`, a
//! workflow's check, past runs and graph, the cases and a new workflow; `pack`, the packs';
//! `machine`, what doctor checks of this machine and this project, and what upkeep collects
//! of what runs left; `setup`, a repository's setup. What one piece of work finds that a
//! later one takes stays with the performer, in `held`, under the ticket that found it: a
//! repository's project detected for its setup, which is bound to the repository, with the
//! pack the setup installs once its own work fetches it, both taken by the setup's write; and
//! a pack fetched, which its install vendors. The state hands back only the ticket, and lets
//! it go when the person leaves the stage that holds it.

mod evidence;
mod following;
mod held;
mod machine;
mod pack;
mod run;
mod setup;
mod workflow;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::mpsc::UnboundedSender;
use yunta_api::{Error, Project, Remark, Remarks, Stop, Workspace};
use yunta_core::events::Channel;

use crate::{Input, News, Operation, Outcome, RepositoryReading, Request, Ticket};
use following::FollowingTask;
use held::Held;

/// Carries out an [`Operation`] through `yunta-api`, against the workspace it was made
/// with, and hands back what it says on the way and what it comes to as inputs.
pub struct Performer {
    workspace: Workspace,
    /// The channel a person reaches it through, which every answer it records carries.
    channel: Channel,
    /// What it holds for the work it was handed, by ticket.
    held: Mutex<Held>,
    /// Its one following, once it was asked to follow.
    following: FollowingTask,
}

impl Performer {
    /// A performer working through `workspace`, which records every answer under
    /// `channel`, the one a person reaches the surface that makes it through.
    pub fn new(workspace: Workspace, channel: Channel) -> Self {
        Performer {
            workspace,
            channel,
            held: Mutex::default(),
            following: FollowingTask::default(),
        }
    }

    /// The workspace it works through, whose project names the store it follows.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The work under `ticket` held as under way, and the stop that stops it: held before
    /// the work starts, so letting it go the moment after it was asked stops it.
    fn under_way(&self, ticket: Ticket) -> Stop {
        let stop = Stop::new();
        self.held().started(ticket, stop.clone());
        stop
    }

    /// `operation` carried out under `ticket`, its work held as under way already under
    /// `stop`: what it says and what lasting work finds go to `inputs` as they come, and
    /// what it came to last.
    async fn performed(
        &self,
        ticket: Ticket,
        operation: Operation,
        inputs: UnboundedSender<Input>,
        stop: Stop,
    ) {
        let remarks = |remark: Remark| {
            // best-effort: a state that went away no longer reads what work says.
            let _ = inputs.send(Input::Remark(ticket, remark));
        };
        let found = |news: News| {
            // best-effort: a state that went away no longer reads what work finds.
            let _ = inputs.send(Input::News(ticket, news));
        };
        let carried_out = self.carried_out(ticket, operation, &remarks, &found, &stop);
        let outcome = match carried_out.await {
            Ok(outcome) => outcome,
            Err(Error::Refused(refused)) => Outcome::Refused(refused),
            Err(Error::Failed(failure)) => Outcome::Failed(failure.to_string()),
            Err(Error::Stopped) => Outcome::Stopped,
        };
        self.held().ended(ticket);
        // best-effort: a state that went away no longer reads what work came to.
        let _ = inputs.send(Input::Outcome(ticket, outcome));
    }

    /// Takes `request`: work to do is held as under way, then starts under its ticket in
    /// a task of its own, and hands what it comes to to `inputs`; work let go is stopped.
    /// Work that breaks inside Yunta still answers its ticket, as failed. What to follow
    /// goes to its one following, which hands what it reads to `inputs`. What only a
    /// surface can do, showing a page, calling the person or leaving, comes back.
    pub fn take(
        self: &Arc<Self>,
        request: Request,
        inputs: &UnboundedSender<Input>,
    ) -> Option<Request> {
        match request {
            Request::Perform(ticket, operation) => {
                let stop = self.under_way(ticket);
                let (performer, sent) = (Arc::clone(self), inputs.clone());
                let work = tokio::spawn(async move {
                    performer.performed(ticket, operation, sent, stop).await;
                });
                let (performer, inputs) = (Arc::clone(self), inputs.clone());
                tokio::spawn(async move {
                    let Err(broke) = work.await else {
                        return;
                    };
                    performer.held().ended(ticket);
                    let said = yunta_render::screen::broke_inside(&broken_words(broke));
                    // best-effort: a state that went away no longer reads what work came to.
                    let _ = inputs.send(Input::Outcome(ticket, Outcome::Failed(said)));
                });
                None
            }
            Request::LetGo(ticket) => {
                self.let_go(ticket);
                None
            }
            Request::Follow(followed) => {
                self.following.follow(&self.workspace, followed, inputs);
                None
            }
            Request::ShowPage(_) | Request::CallThePerson | Request::Leave => Some(request),
        }
    }

    /// Stops the work under `ticket`, if it is under way, and drops what is held for it.
    pub fn let_go(&self, ticket: Ticket) {
        self.held().let_go(ticket);
    }

    /// What `operation`, under `ticket`, comes to, through the API, what it says on the
    /// way said through `remarks` and what lasting work finds handed to `found`.
    async fn carried_out(
        &self,
        ticket: Ticket,
        operation: Operation,
        remarks: &dyn Remarks,
        found: &(dyn Fn(News) + Sync),
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let project = self.workspace.project()?;
        match operation {
            Operation::ReadRepository => repository_read(&project, stop).await,
            Operation::DetectProject => self.detected(&project, ticket, stop).await,
            Operation::FetchSetupPack => self.setup_pack(&project, ticket, stop).await,
            writing @ Operation::WriteSetup { .. } => self.written(writing, remarks, stop).await,
            Operation::ReadCatalog => {
                let catalog = project.workspace.catalog().await;
                Ok(Outcome::Catalog(Box::new(catalog)))
            }
            Operation::Workflow(work) => {
                workflow::performed(&project, work, remarks, found, stop).await
            }
            Operation::Pack(work) => {
                self.pack_performed(&project, ticket, work, remarks, stop)
                    .await
            }
            Operation::Evidence(work) => evidence::performed(&project, work, remarks).await,
            Operation::Doctor { sessions } => {
                machine::doctor(&project, sessions, remarks, stop).await
            }
            Operation::Collect { dry_run } => {
                machine::collected(&project, dry_run, remarks, stop).await
            }
            Operation::ReadPullRequest { run } => run::pull_request_read(&project, &run).await,
            Operation::ReadHistory { workflow } => {
                workflow::history_read(&project, &workflow).await
            }
            Operation::ReadGraph { workflow } => workflow::graph_read(&project, &workflow),
            Operation::Decide { run, option, words } => {
                run::decided(&project, run, option, words).await
            }
            Operation::Answer { run, node, answers } => {
                run::answered(&project, run, &node, answers, self.channel).await
            }
            Operation::Resume { run } => run::resumed(&project, run).await,
            Operation::Cancel { run } => run::cancelled(&project, run, remarks, stop).await,
            Operation::Close { runs } => run::closed(&project, runs, remarks, stop).await,
            Operation::Start { request } => run::started(&project, request, remarks, stop).await,
        }
    }

    /// What it holds, whatever a panicking holder left it as.
    fn held(&self) -> MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The repository `project` works in, as far as Yunta goes, with where the project keeps
/// its runs, what it prices tokens at and the person's home.
async fn repository_read(project: &Project, stop: &Stop) -> Result<Outcome, Error> {
    Ok(Outcome::Repository(Box::new(RepositoryReading {
        repository: project.repository(stop).await?,
        storage: project.storage_path.clone(),
        prices: project.config.pricing.clone(),
        home: project.workspace.env.home.clone(),
    })))
}

/// What the work that broke said as it broke: its panic's message, where it gave one.
fn broken_words(broke: tokio::task::JoinError) -> String {
    let Ok(panic) = broke.try_into_panic() else {
        return String::new();
    };
    match panic.downcast::<String>() {
        Ok(said) => *said,
        Err(panic) => panic
            .downcast::<&str>()
            .map(|said| (*said).to_string())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests;
