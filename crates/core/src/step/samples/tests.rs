use std::collections::BTreeSet;

use super::every_step;
use crate::step::NextStep;

/// How many kinds of step [`kind`] numbers: one per variant of `NextStep`.
const KINDS: usize = 34;

/// Each kind of step by a number of its own. A kind added to `NextStep` does not
/// build here until it has one, and the test then asks for a sample of it.
fn kind(step: &NextStep) -> usize {
    match step {
        NextStep::Status { .. } => 0,
        NextStep::Follow { .. } => 1,
        NextStep::Resume { .. } => 2,
        NextStep::Cancel { .. } => 3,
        NextStep::Close { .. } => 4,
        NextStep::Review { .. } => 5,
        NextStep::Open { .. } => 6,
        NextStep::Receipt { .. } => 7,
        NextStep::Verify { .. } => 8,
        NextStep::ResolveGate { .. } => 9,
        NextStep::Run { .. } => 10,
        NextStep::Check { .. } => 11,
        NextStep::List => 12,
        NextStep::ListRuns { .. } => 13,
        NextStep::Schemas => 14,
        NextStep::Schema { .. } => 15,
        NextStep::Doctor { .. } => 16,
        NextStep::Test { .. } => 17,
        NextStep::Init => 18,
        NextStep::PackAdd { .. } => 19,
        NextStep::PackUpdate { .. } => 20,
        NextStep::PackRemove { .. } => 21,
        NextStep::PackNew { .. } => 22,
        NextStep::Gc { .. } => 23,
        NextStep::Help => 24,
        NextStep::ShowCommit { .. } => 25,
        NextStep::Needs => 26,
        NextStep::Stats { .. } => 27,
        NextStep::History { .. } => 28,
        NextStep::Graph { .. } => 29,
        NextStep::RunGraph { .. } => 30,
        NextStep::NewWorkflow { .. } => 31,
        NextStep::PackList => 32,
        NextStep::PackAudit { .. } => 33,
    }
}

/// Every kind of step has a sample, so a test that walks them walks every kind.
#[test]
fn every_kind_of_step_has_a_sample() {
    let covered: BTreeSet<usize> = every_step().iter().map(kind).collect();
    assert_eq!(covered, (0..KINDS).collect());
}
