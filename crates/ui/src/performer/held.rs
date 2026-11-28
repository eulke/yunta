//! See [`super`]. What the performer holds for the work it was handed, by ticket: the
//! stop of each piece of work under way, and what a piece of work found that later
//! work takes. Both stand under one lock, so a ticket let go while its work ends never
//! leaves anything behind: what work finds is kept only while its ticket is held.
//!
//! A setup holds its project under the ticket that detected it, and the pack it installs
//! beside it. The two are fetched and detected at once, so they are joined here, where both
//! copies are, by whichever lands second: a pack that lands first waits under its fetch's
//! ticket for the project, which takes it as it lands; a pack that lands second joins the
//! project held. A performer works in one repository, so it sets up one at a time, and the
//! one project held without a pack is the one a pack joins. Letting the setup go lets both
//! go, and its write takes both.

use std::collections::BTreeMap;

use yunta_api::{DetectedProject, FetchedPack, Stop};

use crate::Ticket;

/// What the performer holds for the work it was handed, by ticket.
#[derive(Default)]
pub(super) struct Held {
    /// The stop of each piece of work under way.
    stops: BTreeMap<Ticket, Stop>,
    /// What each piece of work found that later work takes, kept under the ticket
    /// that found it until that work takes it or the ticket is let go.
    found: BTreeMap<Ticket, Found>,
}

/// What a piece of work found that later work takes.
pub(super) enum Found {
    /// A repository's project as setting it up detected it, which its setup writes, and
    /// the pack the setup installs once it is fetched, its copy the one its audit read:
    /// held together under the ticket that detected the project, so a setup let go lets
    /// both go.
    Setup {
        project: Box<DetectedProject>,
        pack: Option<Box<FetchedPack>>,
    },
    /// The pack a setup installs, fetched before the project it is held with was detected:
    /// kept under the fetch's ticket until the detection lands and takes it.
    SetupPack(Box<FetchedPack>),
    /// A pack fetched, its copy the one its audit read, which its install vendors.
    Pack(Box<FetchedPack>),
}

impl Held {
    /// The work under `ticket` under way, stopped by `stop`.
    pub(super) fn started(&mut self, ticket: Ticket, stop: Stop) {
        self.stops.insert(ticket, stop);
    }

    /// The work under `ticket` ended: its stop is no longer held, and what it found and
    /// kept stays.
    pub(super) fn ended(&mut self, ticket: Ticket) {
        self.stops.remove(&ticket);
    }

    /// Whether the work under `ticket` is under way: started, and neither ended nor let
    /// go.
    pub(super) fn under_way(&self, ticket: Ticket) -> bool {
        self.stops.contains_key(&ticket)
    }

    /// `found` kept under `ticket` while its work is under way, and whether it was:
    /// nothing is kept for work whose ticket was let go first, which nobody waits on.
    pub(super) fn keep(&mut self, ticket: Ticket, found: Found) -> bool {
        let under_way = self.under_way(ticket);
        if under_way {
            self.found.insert(ticket, found);
        }
        under_way
    }

    /// `project`, detected under `ticket` for its setup, kept with the pack a setup installs
    /// where its fetch landed first, which it takes; whether it was kept, as [`Held::keep`]
    /// says.
    pub(super) fn detected_for_the_setup(
        &mut self,
        ticket: Ticket,
        project: DetectedProject,
    ) -> bool {
        if !self.under_way(ticket) {
            return false;
        }
        let waiting = (self.found.iter())
            .find_map(|(at, found)| matches!(found, Found::SetupPack(_)).then_some(*at));
        let pack = match waiting.and_then(|at| self.found.remove(&at)) {
            Some(Found::SetupPack(pack)) => Some(pack),
            Some(_) | None => None,
        };
        let project = Box::new(project);
        self.found.insert(ticket, Found::Setup { project, pack });
        true
    }

    /// `fetched`, the pack a setup installs fetched under `ticket`: held with the project its
    /// setup detected where that landed first, else kept under `ticket` until it does; whether
    /// it was kept, as [`Held::keep`] says.
    pub(super) fn fetched_for_the_setup(&mut self, ticket: Ticket, fetched: FetchedPack) -> bool {
        if !self.under_way(ticket) {
            return false;
        }
        let held = self.found.values_mut().find_map(|found| match found {
            Found::Setup {
                pack: pack @ None, ..
            } => Some(pack),
            _ => None,
        });
        match held {
            Some(pack) => *pack = Some(Box::new(fetched)),
            None => {
                self.found
                    .insert(ticket, Found::SetupPack(Box::new(fetched)));
            }
        }
        true
    }

    /// The project detected under `ticket`, and the pack fetched with it where one was,
    /// taken: none where it was let go, or what the ticket found is no setup.
    pub(super) fn take_setup(
        &mut self,
        ticket: Ticket,
    ) -> Option<(DetectedProject, Option<FetchedPack>)> {
        match self.found.remove(&ticket)? {
            Found::Setup { project, pack } => Some((*project, pack.map(|pack| *pack))),
            other => {
                self.found.insert(ticket, other);
                None
            }
        }
    }

    /// The pack fetched under `ticket`, taken: none where it was let go, or what the
    /// ticket found is no pack.
    pub(super) fn take_pack(&mut self, ticket: Ticket) -> Option<FetchedPack> {
        match self.found.remove(&ticket)? {
            Found::Pack(fetched) => Some(*fetched),
            other => {
                self.found.insert(ticket, other);
                None
            }
        }
    }

    /// The work under `ticket` stopped, where it is under way, and what it found dropped.
    pub(super) fn let_go(&mut self, ticket: Ticket) {
        if let Some(stop) = self.stops.remove(&ticket) {
            stop.stop();
        }
        self.found.remove(&ticket);
    }
}
