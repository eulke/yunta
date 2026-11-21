//! See [`super`]. What a stage is to the run it stands over ([`ItsRun`]): a view of the run
//! as it moves, which a successor that takes the run's place takes over too, or a record of
//! what was written for the run, which stays that run's.

use yunta_core::RunId;

use super::Stage;

/// What a stage is to its run. Not the run the state follows, which the run's stage under
/// every stage names ([`crate::State::followed`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItsRun {
    /// It shows the run as it moves: the run's screen, its whole chronicle, its numbers, its
    /// graph. Where a successor takes the run's place, it shows the successor.
    Shows(RunId),
    /// It holds what was written for the run: its receipt, its verification, its page for a
    /// browser, each true of that run. Where a successor takes the run's place under it, it
    /// keeps naming that run, and going back finds the successor's screen.
    WroteFor(RunId),
}

impl Stage {
    /// What this stage is to its run; none for a stage over no one run.
    pub(crate) fn its_run(&self) -> Option<ItsRun> {
        match self {
            Stage::Run(run) => Some(ItsRun::Shows(run.run.clone())),
            Stage::Chronicle(chronicle) => Some(chronicle.its_run()),
            Stage::Stats(stats) => stats.its_run(),
            Stage::Graph(graph) => graph.its_run(),
            Stage::Evidence(evidence) => Some(evidence.its_run()),
            Stage::Signpost
            | Stage::Field(_)
            | Stage::FirstRun(_)
            | Stage::Start(_)
            | Stage::Setup(_)
            | Stage::Catalog(_)
            | Stage::Doctor(_)
            | Stage::Upkeep(_)
            | Stage::Schema(_) => None,
        }
    }
}
