//! See [`crate`]. What carries out the work the application asks for, through
//! `yunta-api`, against the workspace it was made with: each operation under a stop of
//! its own, what it says and what it comes to handed back as inputs.
//!
//! A surface makes one performer and hands it every request the state asks for
//! ([`Performer::take`]): work to do starts under its ticket, work let go is stopped,
//! and what only a surface can do, showing a page or leaving, comes back to it. The
//! performer reads nothing of its process: everything comes from the workspace, so a
//! test makes it as it makes a workspace.
//!
//! Each family of work is carried out in a module of its own: `run`, a run's
//! readings, its stats and its graph among them, and its acts; `evidence`, what a run
//! leaves; `workflow`, a workflow's check, past runs and graph, the cases and a new
//! workflow; `pack`, the packs'; `machine`, what doctor checks of this machine and
//! this project, and what upkeep collects of what runs left. What one piece of work finds that a later one takes stays
//! with the performer, in `held`, under the ticket that found it: a repository's project detected for its
//! setup, which is bound to the repository and written by the setup's own work. The
//! state hands back only the ticket, and lets it go when the person leaves the stage
//! that holds it.

mod evidence;
mod held;
mod machine;
mod pack;
mod run;
mod workflow;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::mpsc::UnboundedSender;
use yunta_api::{Error, Project, ProjectSetup, Remark, Remarks, Stop, Workspace};

use crate::{Input, News, Operation, Outcome, Request, Ticket};
use held::{Found, Held};

/// Carries out an [`Operation`] through `yunta-api`, against the workspace it was made
/// with, and hands back what it says on the way and what it comes to as inputs.
pub struct Performer {
    workspace: Workspace,
    /// What it holds for the work it was handed, by ticket.
    held: Mutex<Held>,
}

impl Performer {
    /// A performer working through `workspace`.
    pub fn new(workspace: Workspace) -> Self {
        Performer {
            workspace,
            held: Mutex::default(),
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
        let stop = self.under_way(ticket);
        self.performed(ticket, operation, inputs, stop).await;
    }

    /// The work under `ticket` held as under way, and the stop that stops it: held before
    /// the work starts, so letting it go the moment after it was asked stops it.
    fn under_way(&self, ticket: Ticket) -> Stop {
        let stop = Stop::new();
        self.held().started(ticket, stop.clone());
        stop
    }

    /// [`Performer::perform`], its work held as under way already under `stop`.
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
    /// Work that breaks inside Yunta still answers its ticket, as failed. What only a
    /// surface can do, showing a page or leaving, comes back.
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
            Request::ShowPage(_) | Request::Leave => Some(request),
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
            Operation::ReadRepository => Ok(Outcome::Repository(project.repository(stop).await?)),
            Operation::DetectProject => self.detected(&project, ticket, stop).await,
            Operation::WriteSetup { held, setup } => self.written(held, setup).await,
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
            Operation::Field { reach } => run::field_read(&project, reach, stop).await,
            Operation::Follow { run } => run::followed(&project, &run, found, stop).await,
            Operation::ReadPage { run } => run::page_read(&project, &run, remarks).await,
            Operation::ReadNodePage { run, node } => {
                run::node_page_read(&project, &run, &node, remarks).await
            }
            Operation::ReadQuestions { run } => run::questions_read(&project, &run).await,
            Operation::ReadDecision { run } => run::decision_read(&project, &run).await,
            Operation::ReadStats { run } => run::stats_read(&project, &run).await,
            Operation::ReadRunGraph { run } => run::graph_read(&project, run).await,
            Operation::ReadHistory { workflow } => {
                workflow::history_read(&project, &workflow).await
            }
            Operation::ReadGraph { workflow } => workflow::graph_read(&project, &workflow),
            Operation::Decide { run, option, words } => {
                run::decided(&project, run, option, words).await
            }
            Operation::Answer { run, node, answers } => {
                run::answered(&project, run, &node, answers).await
            }
            Operation::Resume { run } => run::resumed(&project, run).await,
            Operation::Cancel { run } => run::cancelled(&project, run, remarks, stop).await,
            Operation::Close { runs } => run::closed(&project, runs, remarks, stop).await,
            Operation::Start { request } => run::started(&project, request, remarks, stop).await,
        }
    }

    /// `project`'s repository as setting it up detects it, the project kept under
    /// `ticket` for the setup to write; stopped where the ticket was let go first.
    async fn detected(
        &self,
        project: &Project,
        ticket: Ticket,
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let detected = project.workspace.detect_project(false, stop).await?;
        let draft = Box::new(detected.draft.clone());
        match self.held().keep(ticket, Found::Project(Box::new(detected))) {
            true => Ok(Outcome::Detected {
                draft,
                held: ticket,
            }),
            false => Err(Error::Stopped),
        }
    }

    /// `setup` written into the repository whose project is held under `held`; failed
    /// where it was let go first.
    async fn written(&self, held: Ticket, setup: ProjectSetup) -> Result<Outcome, Error> {
        let detected = self.held().take_project(held);
        let Some(detected) = detected else {
            return Ok(Outcome::Failed(
                yunta_render::screen::SETUP_LET_GO.to_string(),
            ));
        };
        let initialized = detected.write(setup).await?;
        Ok(Outcome::Initialized(Box::new(initialized)))
    }

    /// What it holds, whatever a panicking holder left it as.
    fn held(&self) -> MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
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
