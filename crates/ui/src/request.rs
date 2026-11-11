//! See [`crate`]. What the application asks for after an input.

use std::path::PathBuf;

use crate::{Operation, Ticket};

/// What the application asks for after an input: work the surface hands its
/// performer, work it no longer wants, a page to show, or to leave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Work Yunta does, which the surface hands its performer under the ticket.
    Perform(Ticket, Operation),
    /// The state no longer wants this work: stop it if it is under way, and drop what
    /// the performer holds for it.
    LetGo(Ticket),
    /// A page a browser reads: a terminal hands it to the system's opener, the desktop
    /// shows it in a window.
    ShowPage(PathBuf),
    /// The person leaves the application.
    Leave,
}
