//! See [`super`]. What the performer holds for the work it was handed, by ticket: the
//! stop of each piece of work under way, and what a piece of work found that later
//! work takes. Both stand under one lock, so a ticket let go while its work ends never
//! leaves anything behind: what work finds is kept only while its ticket is held.

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
    /// A repository's project as setting it up detected it, which its setup writes.
    Project(Box<DetectedProject>),
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

    /// The project detected under `ticket`, taken: none where it was let go, or what
    /// the ticket found is no project.
    pub(super) fn take_project(&mut self, ticket: Ticket) -> Option<DetectedProject> {
        match self.found.remove(&ticket)? {
            Found::Project(detected) => Some(*detected),
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
