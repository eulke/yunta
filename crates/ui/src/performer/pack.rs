//! See [`super`]. The work the packs ask for: the listing, an audit, a removal, a
//! new pack, and a pack fetched then installed or updated, each as its `yunta pack`
//! command does it. A fetched pack's copy is held under the fetch's ticket, so its
//! install vendors the commit its audit read, never fetched again.

use std::sync::Arc;

use yunta_api::{Error, FetchedPack, Project, Remarks, Stop};
use yunta_core::step::{Arg, NextStep};
use yunta_core::PackRef;
use yunta_render::refusal::Refused;

use super::held::Found;
use super::Performer;
use crate::{
    Outcome, PackFetched, PackInstalled, PackOperation, PackOutcome, PackScaffolded, Ticket,
};

impl Performer {
    /// What `work`, under `ticket`, comes to, through the API, what it says said
    /// through `remarks`.
    pub(super) async fn pack_performed(
        &self,
        project: &Project,
        ticket: Ticket,
        work: PackOperation,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let packs = project.workspace.packs();
        let outcome = match work {
            PackOperation::Fetch { source } => {
                let fetched = packs.fetch(&source, false, stop).await?;
                let installs_as = NextStep::PackAdd {
                    source: Arg::Given(source),
                };
                self.kept_fetched(ticket, fetched, installs_as)?
            }
            PackOperation::FetchUpdate { pack, reference } => {
                let fetched = packs.fetch_update(&pack, &reference, false, stop).await?;
                let installs_as = NextStep::PackUpdate {
                    pack,
                    reference: Arg::Given(reference),
                };
                self.kept_fetched(ticket, fetched, installs_as)?
            }
            PackOperation::Install { held, confirmed } => {
                let fetched = self.held().take_pack(held);
                let Some(fetched) = fetched else {
                    let said = yunta_render::screen::PACK_LET_GO.to_string();
                    return Ok(Outcome::Failed(said));
                };
                installed(project, fetched, confirmed, remarks, stop).await?
            }
            other => other_performed(project, other, remarks, stop).await?,
        };
        Ok(Outcome::Pack(outcome))
    }

    /// `fetched` kept under `ticket` for its install, and what a person confirms
    /// that install against, which `installs_as` does at a shell; stopped where the
    /// ticket was let go first.
    fn kept_fetched(
        &self,
        ticket: Ticket,
        fetched: FetchedPack,
        installs_as: NextStep,
    ) -> Result<PackOutcome, Error> {
        let seen = PackFetched {
            held: ticket,
            installs_as,
            pack: fetched.manifest.reference(),
            audit: fetched.audit(),
            waits_on_yes: fetched.waits_on_yes(),
            updating_to: fetched.updating_to().map(str::to_string),
            checkout: fetched.checkout().to_path_buf(),
        };
        match self.held().keep(ticket, Found::Pack(Box::new(fetched))) {
            true => Ok(PackOutcome::Fetched(Arc::new(seen))),
            false => Err(Error::Stopped),
        }
    }
}

/// The pack `fetched` installed from its copy, with what it executes `confirmed`: an
/// update said as `yunta pack update` says it, and a pack new to the repository as
/// `yunta pack add` does, its cases counted and what its workflows need read.
async fn installed(
    project: &Project,
    fetched: FetchedPack,
    confirmed: bool,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<PackOutcome, Error> {
    let fetched = match confirmed {
        true => fetched.confirmed(),
        false => fetched,
    };
    let updating_to = fetched.updating_to().map(str::to_string);
    let installed = fetched.install(remarks, stop).await?;
    if let Some(to) = updating_to {
        let (pack, commit) = (installed.pack, installed.commit);
        return Ok(PackOutcome::Updated { pack, to, commit });
    }
    let needs = installed
        .needs(project, stop)
        .await
        .map_err(|unread| unread.to_string());
    Ok(PackOutcome::Installed(Arc::new(PackInstalled {
        at: project.workspace.shown(&installed.dir),
        cases: installed.shipped_cases(),
        needs,
        manifest: installed.manifest,
        commit: installed.commit,
    })))
}

/// What the rest of the packs' work comes to: the listing, an audit, a removal and
/// a new pack, none of which holds anything.
async fn other_performed(
    project: &Project,
    work: PackOperation,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<PackOutcome, Error> {
    let packs = project.workspace.packs();
    let outcome = match work {
        PackOperation::List => PackOutcome::Listed(Arc::new(packs.list()?)),
        PackOperation::Audit { pack } => {
            let audited = Arc::new(packs.audit(&pack, remarks, stop).await?);
            PackOutcome::Audited { pack, audited }
        }
        PackOperation::Remove { pack } => {
            packs.remove(&pack, remarks, stop).await?;
            PackOutcome::Removed { pack }
        }
        PackOperation::Scaffold { pack } => pack_scaffolded(project, &pack, remarks, stop).await?,
        PackOperation::Fetch { .. }
        | PackOperation::FetchUpdate { .. }
        | PackOperation::Install { .. } => return Err(Error::Stopped),
    };
    Ok(outcome)
}

/// The new pack `typed` names written and checked, as `yunta pack new` does: refused
/// where it names no pack or a directory stands there already. What stops its
/// check is said beside it, its files written whatever did.
async fn pack_scaffolded(
    project: &Project,
    typed: &str,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<PackOutcome, Error> {
    let pack: PackRef = typed.parse().map_err(Refused::InvalidId)?;
    let scaffolded = project.workspace.packs().scaffold(&pack).await?;
    let checked = scaffolded
        .check(remarks, stop)
        .await
        .map_err(|unchecked| unchecked.to_string());
    Ok(PackOutcome::Scaffolded(Arc::new(PackScaffolded {
        at: project.workspace.shown(&scaffolded.dir),
        pack,
        checked,
    })))
}
