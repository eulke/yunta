//! See [`crate`]. What a piece of work came to, as the performer hands it back.

use std::path::PathBuf;
use std::sync::Arc;

use surco::components::Check;
use yunta_api::{
    AuditedPack, FieldReading, HistoryReading, InitializedProject, ProjectDraft, Repository,
    StatsReading, WaitingDecision, WaitingQuestions, WorkflowCheck,
};
use yunta_core::receipt::Receipt;
use yunta_core::step::NextStep;
use yunta_core::{CommitSha, PackAudit, PackManifest, PackRef, RunId, WorkflowShape};
use yunta_render::page::doctor::Finding;
use yunta_render::page::pack::{Cases, Listed, Needs, NewPackCheck};
use yunta_render::page::Collected;
use yunta_render::page::{CancelState, Catalog, Diagnosis, Graph, Node, Run, Tested};
use yunta_render::refusal::Refused;

use crate::Ticket;

/// What an [`Operation`](crate::Operation) came to: one variant per operation's data,
/// and three for one that did not do what it was asked.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The repository the application works in, as far as Yunta goes.
    Repository(Repository),
    /// What setting the repository up found, and the ticket the performer holds the
    /// project detected under, which its setup is written with.
    Detected {
        draft: Box<ProjectDraft>,
        held: Ticket,
    },
    /// What setting the repository up wrote, and what it left as it was.
    Initialized(Box<InitializedProject>),
    /// The workflows the repository can run.
    Catalog(Box<Catalog>),
    /// What work on a workflow came to.
    Workflow(WorkflowOutcome),
    /// What work on the packs came to.
    Pack(PackOutcome),
    /// What a run leaves.
    Evidence(EvidenceOutcome),
    /// The runs read, and where the project keeps them.
    Field {
        reading: FieldReading,
        storage: PathBuf,
    },
    /// The run no longer moves, and its following ended.
    Followed,
    /// A run's page.
    Page(Box<Run>),
    /// The page of a run's node.
    NodePage(Box<Node>),
    /// The round of questions a run is parked on.
    Questions(Box<WaitingQuestions>),
    /// The decision a run waits on, with the documents it shows.
    Decision(Box<WaitingDecision>),
    /// What a run spent.
    Stats(Box<StatsReading>),
    /// A workflow's past runs, and what their logs say of its checks.
    History(Box<HistoryReading>),
    /// A workflow's graph, each node labelled with a run's state where it is a run's.
    Graph(Box<Graph>),
    /// What every check doctor made found, in the order it made them.
    Doctor(Vec<Finding>),
    /// What a collection reclaimed, or a dry run would.
    Collected(Box<Collected>),
    /// The decision recorded, and the run handed on to a detached resume.
    Decided { run: RunId },
    /// The answers recorded, and the run handed on to a detached resume.
    Answered { run: RunId },
    /// The run started detached.
    Started { run: RunId },
    /// The run handed back to a detached resume.
    Resumed { run: RunId },
    /// How cancelling the run ended.
    Cancelled { run: RunId, state: CancelState },
    /// The runs closed, why each of the others was not, and what failed below Yunta
    /// and stopped the closing short, where something did: the runs closed before it
    /// stand closed whatever came after.
    Closed {
        closed: Vec<RunId>,
        refused: Vec<Refused>,
        failed: Option<String>,
    },
    /// What Yunta refused, which render says.
    Refused(Refused),
    /// What failed below Yunta, in its own words.
    Failed(String),
    /// The work was stopped before it ended.
    Stopped,
}

/// What work on a workflow came to: a [`WorkflowOperation`](crate::WorkflowOperation)'s
/// data.
#[derive(Debug, Clone)]
pub enum WorkflowOutcome {
    /// What checking it came to, as `yunta check` says it.
    Checked(Box<WorkflowCheck>),
    /// A new workflow written, as `yunta new` says it.
    Written(Box<NewWorkflowWritten>),
    /// How many cases ran, and how many did not hold.
    Tested(Tested),
}

/// What work on the packs came to: a [`PackOperation`](crate::PackOperation)'s data.
/// What is read whole and never changed, a listing, an audit, a new pack's page, is
/// shared.
#[derive(Debug, Clone)]
pub enum PackOutcome {
    /// The packs installed.
    Listed(Arc<Listed>),
    /// An installed pack's audit, and what its own cases did.
    Audited {
        pack: PackRef,
        audited: Arc<AuditedPack>,
    },
    /// The pack removed.
    Removed { pack: PackRef },
    /// A new pack written, as `yunta pack new` says it.
    Scaffolded(Arc<PackScaffolded>),
    /// A pack fetched, its copy held.
    Fetched(Arc<PackFetched>),
    /// A pack installed, as `yunta pack add` says it.
    Installed(Arc<PackInstalled>),
    /// A pack updated, as `yunta pack update` says it: to the ref `to`, at
    /// `commit`.
    Updated {
        pack: PackRef,
        to: String,
        commit: CommitSha,
    },
}

/// What a run leaves: an [`EvidenceOperation`](crate::EvidenceOperation)'s data.
#[derive(Debug, Clone)]
pub enum EvidenceOutcome {
    /// The receipt, as it was written beside the run.
    Receipt(Box<Receipt>),
    /// Each check of `run`'s evidence, in the order it ran.
    Verified { run: RunId, checks: Vec<Check> },
    /// Where the run's page for a browser was written.
    PageWritten { page: PathBuf },
}

/// A new workflow as `yunta new` says it: its name and the shape its skeleton
/// took, where it was written and as a reader in the repository names it, and the
/// verdict `check` reaches on it, or what stopped that: the file is written
/// whatever did.
#[derive(Debug, Clone)]
pub struct NewWorkflowWritten {
    pub name: String,
    pub shape: WorkflowShape,
    pub path: PathBuf,
    pub at: String,
    pub diagnosis: Result<Diagnosis, String>,
}

/// A new pack as `yunta pack new` says it: the pack, where it was written as a
/// reader in the repository names it, and what its check found, or what stopped
/// that: the files are written whatever did.
#[derive(Debug)]
pub struct PackScaffolded {
    pub pack: PackRef,
    pub at: String,
    pub checked: Result<NewPackCheck, String>,
}

/// A pack fetched, as a person confirms its install against it: the ticket the
/// performer holds its copy under, the copy's audit, whether what it executes waits
/// on a person's confirmation, the ref an update moves it to (none for a pack new
/// to the repository), and where the copy stands until it is installed or let go.
#[derive(Debug)]
pub struct PackFetched {
    pub held: Ticket,
    /// The command that fetches and installs it as this did: `yunta pack add` with the
    /// source it was fetched from, or `yunta pack update` with the ref it moves to.
    pub installs_as: NextStep,
    pub pack: PackRef,
    pub audit: PackAudit,
    pub waits_on_yes: bool,
    pub updating_to: Option<String>,
    pub checkout: PathBuf,
}

/// A pack installed, as `yunta pack add` says it: what it declares, the commit it
/// was vendored from, where, as a reader in the repository names it, its own cases
/// counted, and what its workflows need from the project, or what stopped reading
/// that: the pack is installed whatever did.
#[derive(Debug)]
pub struct PackInstalled {
    pub manifest: PackManifest,
    pub commit: CommitSha,
    pub at: String,
    pub cases: Cases,
    pub needs: Result<Needs, String>,
}
