//! See [`crate`]. What the application asks for after an input.

use std::path::PathBuf;

use yunta_api::Followed;

use crate::{Operation, Ticket};

/// What the application asks for after an input: work the surface hands its
/// performer, work it no longer wants, what to follow, a page to show, the person
/// called, or to leave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Work Yunta does, which the surface hands its performer under the ticket.
    Perform(Ticket, Operation),
    /// The state no longer wants this work: stop it if it is under way, and drop what
    /// the performer holds for it.
    LetGo(Ticket),
    /// What the application follows, handed whole each time it changes, which the
    /// performer's one following follows from then on. It has no ticket: a following is
    /// never let go, only followed anew.
    Follow(Followed),
    /// A page a browser reads: a terminal hands it to the system's opener, the desktop
    /// shows it in a window.
    ShowPage(PathBuf),
    /// A run came to need the person: asked once each time a run the application keeps
    /// turns to need them after the field is first read, never again for news of the
    /// same pause, and never for a run that already needed them as the field it stands in
    /// was read whole. A terminal rings its bell where the person asked for one; the
    /// window's title, which names the run, is the screen's.
    CallThePerson,
    /// The person leaves the application.
    Leave,
}
