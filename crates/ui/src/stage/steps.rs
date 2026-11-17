//! See [`super`]. The one mapping from a step, as a command line names it, to the stage
//! that takes it: what every stage's palette offers after the stage's own acts, the stage
//! each step opens, and the step whose command shows what a stage shows.

use yunta_core::step::{Arg, NextStep};

use super::{
    CatalogStage, DecisionStage, DoctorStage, Evidence, EvidenceStage, GraphStage, RunStage,
    SchemaStage, Stage, StatsStage, UpkeepStage,
};
use crate::prompt::Opens;

impl Stage {
    /// What every stage's palette offers after the stage's own acts, in their order:
    /// the workflows, doctor, upkeep and the shapes of the documents Yunta reads, each
    /// opening the stage that takes it ([`Stage::opened_by`]).
    pub(crate) fn always() -> [NextStep; 4] {
        [
            NextStep::List,
            NextStep::Doctor {
                sessions: false,
                full: false,
            },
            NextStep::Gc { dry_run: true },
            NextStep::Schemas,
        ]
    }

    /// The stage `step` opens: one of [`Stage::always`]; a run's step that shows or
    /// follows the run, its decision on its card or on its plan, its evidence, its
    /// numbers or its graph; or a workflow's past runs or its graph. None for a step that
    /// is work or asks a confirmation first, which the stage it is taken on carries out.
    /// The one mapping from a step to the stage that takes it: every stage and every
    /// opening reads it.
    pub(crate) fn opened_by(step: &NextStep) -> Option<Stage> {
        Some(match step {
            NextStep::List => Stage::Catalog(CatalogStage::new()),
            NextStep::Doctor {
                sessions: false,
                full: false,
            } => Stage::Doctor(DoctorStage::new()),
            NextStep::Gc { dry_run: true } => Stage::Upkeep(UpkeepStage::new()),
            NextStep::Schemas => Stage::Schema(SchemaStage::new()),
            NextStep::Status { run, .. } | NextStep::Follow { run } => {
                Stage::Run(RunStage::of(run.clone()))
            }
            NextStep::ResolveGate { run, .. } => {
                Stage::Decision(DecisionStage::of(run.clone(), Opens::TheCard))
            }
            NextStep::Review { run } => {
                Stage::Decision(DecisionStage::of(run.clone(), Opens::ThePlan))
            }
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
            Stage::Upkeep(_) => Some(NextStep::Gc { dry_run: true }),
            Stage::Schema(_) => Some(NextStep::Schemas),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::Run(_)
            | Stage::Decision(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Evidence(_)
            | Stage::Stats(_)
            | Stage::Graph(_) => None,
        }
    }
}
