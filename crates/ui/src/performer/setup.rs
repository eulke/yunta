//! See [`super`]. The work a setup asks for: the repository's project detected, with who git
//! commits a run's work as there, and held under the ticket that detected it; the pack a
//! setup installs fetched beside it as the setup opens, work of its own that reaches the
//! network unless the repository vendors the pack already, held with the project by
//! whichever of the two lands second (`held`); and the setup written.
//!
//! The project stays as it was detected while the pack is held with it: the roles the
//! repository would propose once the pack is installed, read from the copy fetched, go back
//! to the setup that asked, and the setup it answers carries the proposal it was answered
//! from, which the write takes its roles and default runner from, so a setup written
//! without the pack writes what `init` writes. Where the write installs it, it installs
//! first, from the copy its audit read, as `yunta pack add --yes` does, then writes the
//! configuration and reads what the installed pack still needs of it: a pack that cannot be
//! installed leaves the repository as it was, and a configuration that cannot be written
//! leaves the pack installed, which the setup then finds there.

use std::sync::Arc;

use yunta_api::{Error, Project, Remarks, SetupPack, Stop};
use yunta_core::refusal::PackRefusal;
use yunta_core::step::{Arg, NextStep};
use yunta_render::refusal::Refused;

use super::pack::said_installed;
use super::Performer;
use crate::{InstallsThePack, Operation, Outcome, SetupPackFetch, Ticket};

impl Performer {
    /// `project`'s repository as setting it up detects it, and who git commits a run's work
    /// as there, the project kept under `ticket` for the setup to write; stopped where the
    /// ticket was let go first.
    pub(super) async fn detected(
        &self,
        project: &Project,
        ticket: Ticket,
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let detected = project.workspace.detect_project(false, stop).await?;
        let committer = project.workspace.committer(stop).await?;
        let draft = Box::new(detected.draft.clone());
        match self.held().detected_for_the_setup(ticket, detected) {
            true => Ok(Outcome::Detected {
                draft,
                held: ticket,
                committer,
            }),
            false => Err(Error::Stopped),
        }
    }

    /// The pack a setup installs, fetched under `ticket`: installed already, where the
    /// repository vendors it, read with no network asked; fetched, its copy held with the
    /// project its setup detected, with its audit and the roles the repository would propose
    /// once it is installed; or out of reach, with why. Stopped where the ticket was let go
    /// first.
    pub(super) async fn setup_pack(
        &self,
        project: &Project,
        ticket: Ticket,
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let offered = SetupPack::of_this_binary();
        let pack = offered.pack().clone();
        let fetch = match offered.fetched(&project.workspace, stop).await {
            Ok(Some(fetched)) => {
                let (audit, proposed) = (Box::new(fetched.audit()), fetched.proposed());
                if !self.held().fetched_for_the_setup(ticket, fetched) {
                    return Err(Error::Stopped);
                }
                let installs_as = NextStep::PackAdd {
                    source: Arg::Given(offered.source().to_string()),
                };
                SetupPackFetch::Fetched {
                    pack,
                    installs_as,
                    audit,
                    proposed,
                }
            }
            Ok(None) => SetupPackFetch::Installed { pack },
            Err(Error::Refused(Refused::Pack(refusal)))
                if matches!(*refusal, PackRefusal::AlreadyInstalled { .. }) =>
            {
                SetupPackFetch::Installed { pack }
            }
            Err(Error::Refused(refused)) => SetupPackFetch::OutOfReach {
                pack,
                why: refused.to_string(),
            },
            Err(Error::Failed(failure)) => SetupPackFetch::OutOfReach {
                pack,
                why: failure.in_its_own_words(),
            },
            Err(Error::Stopped) => return Err(Error::Stopped),
        };
        Ok(Outcome::SetupPack {
            fetched: Arc::new(fetch),
            held: ticket,
        })
    }

    /// `writing`'s setup written into the repository whose project is held under its ticket,
    /// the pack held with it installed first where it says so, and what it still needs read
    /// once the configuration is written; the pack let go otherwise; what installing says
    /// said through `remarks`. Failed where the setup was let go first.
    pub(super) async fn written(
        &self,
        writing: Operation,
        remarks: &dyn Remarks,
        stop: &Stop,
    ) -> Result<Outcome, Error> {
        let Operation::WriteSetup { held, setup, pack } = writing else {
            return Err(Error::Stopped);
        };
        let Some((project, fetched)) = self.held().take_setup(held) else {
            return Ok(Outcome::Failed(
                yunta_render::screen::SETUP_LET_GO.to_string(),
            ));
        };
        let installing = match pack {
            InstallsThePack::Yes => fetched,
            InstallsThePack::No => None,
        };
        let installed = match installing {
            Some(fetched) => Some(fetched.confirmed().install(remarks, stop).await?),
            None => None,
        };
        let initialized = Box::new(project.write(setup).await?);
        let installed = match installed {
            Some(installed) => {
                let set_up = self.workspace.project()?;
                Some(Arc::new(said_installed(&set_up, installed, stop).await))
            }
            None => None,
        };
        Ok(Outcome::Initialized {
            initialized,
            installed,
        })
    }
}
