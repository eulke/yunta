//! See [`super`]. The one mapping from a step, as a command line names it, to the stage
//! that takes it: what every stage's palette offers after the stage's own acts, the stage
//! each step opens, and the step whose command shows what a stage shows.

use yunta_api::{FieldReach, Repository};
use yunta_core::step::{Arg, NextStep, Reach};

use super::{
    CatalogStage, ChronicleStage, DoctorStage, Evidence, EvidenceStage, FieldStage, GraphStage,
    RunStage, SchemaStage, Stage, StartStage, StatsStage, UpkeepStage,
};
use crate::prompt::Opens;
use crate::FollowedRuns;

impl Stage {
    /// What every stage's palette offers after the stage's own acts, in their order, of what
    /// works where the application stands as `followed` keeps its repository: in a repository
    /// set up, this repository's runs, the workflows, doctor, upkeep and the shapes of
    /// the documents Yunta reads; in one never set up, all but its runs, since no field stands
    /// under a setup to go down to; outside a repository, doctor, which checks this machine,
    /// and the shapes, nothing else working without one. Each is taken as
    /// [`Want::taking`](super::Want::taking) takes it: the nearest field for the runs, and the
    /// stage that takes any other ([`Stage::opened_by`]).
    pub(crate) fn always(followed: &FollowedRuns) -> Vec<NextStep> {
        let runs = NextStep::ListRuns {
            reach: Reach::Repository,
        };
        let doctor = NextStep::Doctor {
            sessions: false,
            full: false,
        };
        let (workflows, upkeep) = (NextStep::List, NextStep::Gc { dry_run: false });
        match &followed.repository {
            Some(Repository::SetUp { .. }) => {
                vec![runs, workflows, doctor, upkeep, NextStep::Schemas]
            }
            Some(Repository::NeverSetUp { .. }) => {
                vec![workflows, doctor, upkeep, NextStep::Schemas]
            }
            Some(Repository::Absent) => vec![doctor, NextStep::Schemas],
            None => Vec::new(),
        }
    }

    /// The stage `step` opens: one of [`Stage::always`] but the runs, which are the field
    /// every stage goes down to; the field at the machine's reach, which the signpost reaches
    /// outside a repository, where no field stands under it; upkeep for `gc`, whether it removes or says what it would,
    /// since the page says what it would remove and removes it only once confirmed; a run's
    /// step that shows the run, its decision on its card as `status` and `resolve-gate` show
    /// it or on its plan as `review` shows it, its whole chronicle as `--follow` prints it,
    /// its evidence, its numbers or its graph; a workflow's past runs or its graph; or a new
    /// run, blank or of the workflow the step names, which reads what it needs as it opens.
    /// None for a step that is work or asks a confirmation first, which the stage it is taken
    /// on carries out. The one mapping from a step to the stage that takes it: every stage and
    /// every opening reads it.
    pub(crate) fn opened_by(step: &NextStep) -> Option<Stage> {
        Some(match step {
            NextStep::ListRuns {
                reach: Reach::Machine,
            } => Stage::Field(FieldStage::reaching(FieldReach::Machine)),
            NextStep::List => Stage::Catalog(CatalogStage::new()),
            NextStep::Doctor {
                sessions: false,
                full: false,
            } => Stage::Doctor(DoctorStage::new()),
            NextStep::Gc { .. } => Stage::Upkeep(UpkeepStage::new()),
            NextStep::Schemas => Stage::Schema(SchemaStage::new()),
            NextStep::Status { run, .. } | NextStep::ResolveGate { run, .. } => {
                Stage::Run(RunStage::opened_on(run.clone(), Opens::TheCard))
            }
            NextStep::Review { run } => {
                Stage::Run(RunStage::opened_on(run.clone(), Opens::ThePlan))
            }
            NextStep::Follow { run } => Stage::Chronicle(ChronicleStage::of(run.clone())),
            NextStep::Receipt { run } => {
                Stage::Evidence(EvidenceStage::of(run.clone(), Evidence::Receipt))
            }
            NextStep::Verify { run } => {
                Stage::Evidence(EvidenceStage::of(run.clone(), Evidence::Verification))
            }
            NextStep::Open { run } => {
                Stage::Evidence(EvidenceStage::of(run.clone(), Evidence::PageForABrowser))
            }
            NextStep::Stats { run } => Stage::Stats(StatsStage::of_run(run.clone())),
            NextStep::RunGraph { run } => Stage::Graph(GraphStage::of_run(run.clone())),
            NextStep::History { workflow } => {
                Stage::Stats(StatsStage::of_workflow(workflow.clone()))
            }
            NextStep::Graph {
                workflow: Arg::Given(workflow),
            } => Stage::Graph(GraphStage::of_workflow(workflow.clone())),
            NextStep::Run { workflow } => Stage::Start(StartStage::opened(match workflow {
                Arg::Given(workflow) => Some(workflow.clone()),
                Arg::Blank => None,
            })),
            _ => return None,
        })
    }

    /// The step of [`Stage::always`] whose command shows what this stage shows: a
    /// palette over it does not offer opening it again.
    pub(crate) fn showing(&self) -> Option<NextStep> {
        match self {
            Stage::Catalog(_) => Some(NextStep::List),
            Stage::Doctor(_) => Some(NextStep::Doctor {
                sessions: false,
                full: false,
            }),
            Stage::Upkeep(_) => Some(NextStep::Gc { dry_run: false }),
            Stage::Schema(_) => Some(NextStep::Schemas),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Chronicle(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_) => None,
        }
    }
}
