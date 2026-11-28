//! See [`crate`]. What the application asks for after an input.

use std::path::PathBuf;

use yunta_api::Followed;

use crate::{Operation, Ticket};

/// What the application asks for after an input: work the surface hands its
/// performer, work it no longer wants, what to follow, a page or a link to show, the
/// person called, or to leave.
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
    /// A page or a link a browser reads: a terminal hands it to the system's opener, the
    /// desktop shows it in a window.
    ShowPage(Address),
    /// A run came to need the person: asked once each time a run the application keeps
    /// turns to need them after the field is first read, never again for news of the
    /// same pause, and never for a run that already needed them as the field it stands in
    /// was read whole. A terminal rings its bell where the person asked for one; the
    /// window's title, which names the run, is the screen's.
    CallThePerson,
    /// The person leaves the application.
    Leave,
}

/// What a surface is asked to show: a page written on this machine, or a page elsewhere,
/// such as a pull request on its forge. A link is never held as a path, so the two are
/// told apart, and opened alike. Not
/// [`NextStep::Open`](yunta_core::step::NextStep::Open), the command that writes a run's
/// page and shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// A page written on this machine, by its path.
    Page(PathBuf),
    /// A page elsewhere, by its `https://` link.
    Link(WebLink),
}

/// A page elsewhere, by a link that is `https://` and nothing else: the system's opener a
/// surface hands it to launches whatever another scheme names, an application by a
/// `file://` link among them. Not a link as a log holds it, which is untrusted text a
/// screen says and never opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebLink(String);

impl WebLink {
    /// `link` as a page elsewhere, where it is `https://`; none otherwise.
    pub fn https(link: &str) -> Option<WebLink> {
        link.starts_with("https://")
            .then(|| WebLink(link.to_string()))
    }

    /// The link, as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
