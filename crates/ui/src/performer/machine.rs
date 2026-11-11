//! See [`super`]. What doctor checks of this machine and this project, and what
//! upkeep collects of what runs left, through the API's `doctor` and `collect`, which
//! `yunta doctor` and `yunta gc` call.

use yunta_api::{Collect, Error, Project, Remarks, Stop};

use crate::Outcome;

/// What every check doctor makes found; under `sessions`, with one real session per
/// binding. What the checks say while they run is said through `remarks`.
pub(super) async fn doctor(
    project: &Project,
    sessions: bool,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Outcome, Error> {
    let findings = project.doctor(sessions, remarks, stop).await?;
    Ok(Outcome::Doctor(findings))
}

/// What collecting what runs left reclaimed; under `dry_run`, what it would, with
/// nothing removed. What the pass could not do is said through `remarks` as it happens.
pub(super) async fn collected(
    project: &Project,
    dry_run: bool,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Outcome, Error> {
    let collect = match dry_run {
        true => Collect::DryRun,
        false => Collect::Pass,
    };
    let collected = project.collect(collect, remarks, stop).await?;
    Ok(Outcome::Collected(Box::new(collected)))
}
