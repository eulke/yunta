//! See [`crate`]. What a stage asks of Yunta, as data, which the performer carries out
//! through `yunta-api`.
//!
//! One variant per thing a stage asks, named by its verb; each stage that asks
//! something new adds its variant here, and the performer carries it out. The work of
//! a family that grows on its own, a workflow's and a pack's, nests under an enum of
//! its own, which its stage and its part of the performer hold whole: a stage that
//! never asks for it names the family alone. What kind of work an operation is,
//! whatever it names, is its [`OperationKind`].

use std::mem::{discriminant, Discriminant};

use yunta_api::{FieldReach, ProjectSetup, RunRequest};
use yunta_core::{Answer, NodeId, OptionId, PackRef, RunId, WorkflowName, WorkflowShape};

use crate::Ticket;

/// Work a stage asks Yunta to do, named by its verb. Not a command line a person
/// types ([`yunta_core::step::NextStep`]): this is what the application asks the
/// performer, and an answer comes back as an [`Outcome`](crate::Outcome).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// The repository the application works in, as far as Yunta goes: none, one never
    /// set up, or one set up.
    ReadRepository,
    /// What setting the repository up finds: what it runs, the adapters this machine
    /// answers for and the runners its workflows propose. The performer holds the
    /// project detected under this work's ticket, for its setup to write.
    DetectProject,
    /// `setup` written into the repository whose project the performer holds under
    /// `held`, the ticket that detected it.
    WriteSetup { held: Ticket, setup: ProjectSetup },
    /// The workflows the repository can run.
    ReadCatalog,
    /// What is done to a workflow the catalog offers, or to the repository's cases.
    Workflow(WorkflowOperation),
    /// What is done to the packs a project installs.
    Pack(PackOperation),
    /// What a run leaves: its receipt, its verification, its page for a browser.
    Evidence(EvidenceOperation),
    /// The runs as far as `reach` reaches.
    Field { reach: FieldReach },
    /// `run` followed while it moves, onto the successor a promotion hands the work to.
    Follow { run: RunId },
    /// `run`'s page.
    ReadPage { run: RunId },
    /// The page of `run`'s node `node`.
    ReadNodePage { run: RunId, node: NodeId },
    /// The round of questions `run` is parked on.
    ReadQuestions { run: RunId },
    /// The decision `run` waits on, with the documents it shows.
    ReadDecision { run: RunId },
    /// What `run` spent, as `yunta stats` reads it.
    ReadStats { run: RunId },
    /// The past runs of the workflow called `workflow`, as `yunta stats --workflow`
    /// reads them.
    ReadHistory { workflow: WorkflowName },
    /// The workflow the catalog offers as `workflow`, drawn as written, as `yunta
    /// graph` draws it.
    ReadGraph { workflow: String },
    /// The workflow `run` froze, drawn with each node labelled with what the run's
    /// log derives for it, as `yunta graph --run` draws it.
    ReadRunGraph { run: RunId },
    /// `option` recorded on the decision `run` waits on, with the `words` it was chosen
    /// with, and the run handed on to a detached resume.
    Decide {
        run: RunId,
        option: OptionId,
        words: Option<String>,
    },
    /// `answers` to the round `node` asked recorded on `run`, and the run handed on to
    /// a detached resume.
    Answer {
        run: RunId,
        node: NodeId,
        answers: Vec<Answer>,
    },
    /// Hands `run` back to a detached resume.
    Resume { run: RunId },
    /// Cancels `run` and everything under it.
    Cancel { run: RunId },
    /// Closes each of `runs` for good.
    Close { runs: Vec<RunId> },
    /// The run `request` asks for, admitted, created and started detached.
    Start { request: RunRequest },
    /// Every check `yunta doctor` makes of this machine and this project; under
    /// `sessions`, one real session per binding, which spends a prompt each.
    Doctor { sessions: bool },
    /// What runs left collected, as `yunta gc` collects it; under `dry_run`, what
    /// collecting would reclaim, with nothing removed.
    Collect { dry_run: bool },
}

impl Operation {
    /// Whether the operation only reads, so one asked again supersedes one of its kind
    /// still under way, whose answer would be stale by the time it came.
    pub(crate) fn reads(&self) -> bool {
        match self {
            Operation::ReadRepository
            | Operation::DetectProject
            | Operation::ReadCatalog
            | Operation::Field { .. }
            | Operation::Follow { .. }
            | Operation::ReadPage { .. }
            | Operation::ReadNodePage { .. }
            | Operation::ReadQuestions { .. }
            | Operation::ReadDecision { .. }
            | Operation::ReadStats { .. }
            | Operation::ReadHistory { .. }
            | Operation::ReadGraph { .. }
            | Operation::ReadRunGraph { .. } => true,
            Operation::Doctor { sessions } => !sessions,
            Operation::Collect { dry_run } => *dry_run,
            Operation::Workflow(work) => work.reads(),
            Operation::Pack(work) => work.reads(),
            Operation::Evidence(_) => true,
            Operation::Resume { .. }
            | Operation::Cancel { .. }
            | Operation::Close { .. }
            | Operation::Decide { .. }
            | Operation::Answer { .. }
            | Operation::Start { .. }
            | Operation::WriteSetup { .. } => false,
        }
    }

    /// Whether it stops, closes, removes or spends: cancelling a run, closing runs,
    /// removing a pack, installing one whose executors were confirmed, opening a session
    /// per binding, collecting. Such work is asked only once a person confirms it.
    pub fn needs_confirming(&self) -> bool {
        match self {
            Operation::Cancel { .. }
            | Operation::Close { .. }
            | Operation::Pack(PackOperation::Remove { .. })
            | Operation::Pack(PackOperation::Install {
                confirmed: true, ..
            })
            | Operation::Doctor { sessions: true }
            | Operation::Collect { dry_run: false } => true,
            Operation::ReadRepository
            | Operation::DetectProject
            | Operation::WriteSetup { .. }
            | Operation::ReadCatalog
            | Operation::Workflow(_)
            | Operation::Pack(_)
            | Operation::Evidence(_)
            | Operation::Field { .. }
            | Operation::Follow { .. }
            | Operation::ReadPage { .. }
            | Operation::ReadNodePage { .. }
            | Operation::ReadQuestions { .. }
            | Operation::ReadDecision { .. }
            | Operation::ReadStats { .. }
            | Operation::ReadHistory { .. }
            | Operation::ReadGraph { .. }
            | Operation::ReadRunGraph { .. }
            | Operation::Decide { .. }
            | Operation::Answer { .. }
            | Operation::Resume { .. }
            | Operation::Start { .. }
            | Operation::Doctor { sessions: false }
            | Operation::Collect { dry_run: true } => false,
        }
    }

    /// What kind of work it is, whatever it names.
    pub fn kind(&self) -> OperationKind {
        match self {
            Operation::Workflow(work) => OperationKind::Workflow(discriminant(work)),
            Operation::Pack(work) => OperationKind::Pack(discriminant(work)),
            Operation::Evidence(work) => OperationKind::Evidence(discriminant(work)),
            other => OperationKind::Of(discriminant(other)),
        }
    }
}

/// What is done to a workflow the catalog offers, or to the repository's cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowOperation {
    /// The workflow the catalog offers as `workflow` checked, as `yunta check`
    /// checks it.
    Check { workflow: String },
    /// The repository's cases run, each handed back as it ends: what `yunta test`
    /// runs.
    RunCases,
    /// A new workflow called `name` written from the skeleton of `shape` and
    /// checked, as `yunta new` writes it.
    New { name: String, shape: WorkflowShape },
}

impl WorkflowOperation {
    /// Whether it only reads: a check and the cases, which write nothing of the
    /// project's.
    fn reads(&self) -> bool {
        match self {
            WorkflowOperation::Check { .. } | WorkflowOperation::RunCases => true,
            WorkflowOperation::New { .. } => false,
        }
    }
}

/// What is done to the packs a project installs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackOperation {
    /// The packs installed, as `yunta pack list` reads them.
    List,
    /// The installed `pack` audited, its own cases run on it, as `yunta pack
    /// audit` does.
    Audit { pack: PackRef },
    /// The installed `pack` removed, as `yunta pack remove` removes it.
    Remove { pack: PackRef },
    /// A new pack `pack` (a publisher and a name, as typed) written and checked,
    /// as `yunta pack new` does.
    Scaffold { pack: String },
    /// The pack `source` names (`<source>[@<ref>]`) fetched to install it, as
    /// `yunta pack add` fetches it: the performer holds the copy its audit reads
    /// under this work's ticket, for its install to vendor.
    Fetch { source: String },
    /// `reference` of the installed `pack` fetched to update it to, as `yunta pack
    /// update` fetches it, its copy held as a fetch's is.
    FetchUpdate { pack: PackRef, reference: String },
    /// The pack held under `held`, the ticket that fetched it, installed from the
    /// copy its audit read, never fetched again; with what it executes `confirmed`
    /// by the person who read that audit.
    Install { held: Ticket, confirmed: bool },
}

impl PackOperation {
    /// Whether it only reads: the listing, an audit and a fetch, which writes
    /// nothing of the project's.
    fn reads(&self) -> bool {
        match self {
            PackOperation::List
            | PackOperation::Audit { .. }
            | PackOperation::Fetch { .. }
            | PackOperation::FetchUpdate { .. } => true,
            PackOperation::Remove { .. }
            | PackOperation::Scaffold { .. }
            | PackOperation::Install { .. } => false,
        }
    }
}

/// What kind of work an operation is, whatever it names: a reading asked again lets
/// go the one of its kind under way, and a test answers every operation of a kind
/// alike. Not the operation, which names what the work is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    /// Work of no family: the repository's, a run's, the catalog read.
    Of(Discriminant<Operation>),
    Workflow(Discriminant<WorkflowOperation>),
    Pack(Discriminant<PackOperation>),
    Evidence(Discriminant<EvidenceOperation>),
}

/// What a run leaves, derived from its log and written beside it: none of it changes
/// the run or the project, so each is a reading, let go with the stage that asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceOperation {
    /// `run`'s receipt, written beside it as `yunta receipt` writes it.
    WriteReceipt { run: RunId },
    /// `run`'s log and the objects it names verified, as `yunta verify` does.
    Verify { run: RunId },
    /// `run`'s page written where a browser reads it, as `yunta open` writes it.
    WritePageForBrowser { run: RunId },
}
