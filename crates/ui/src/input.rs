//! See [`crate`]. What reaches the application.

use chrono::{DateTime, Utc};
use surco::components::ScreenLaid;
use yunta_api::{FollowNews, Remark};

use crate::prompt::Stroke;
use crate::{Address, News, Outcome, Ticket};

/// What reaches the application: a stroke a person gave, what a surface laid out, the
/// time, what work it asked for says while it works, hands back while it lasts and came
/// to, what it follows and a look at it that failed, and a page the surface could not
/// show.
#[derive(Debug, Clone)]
pub enum Input {
    Stroke(Stroke),
    /// What a surface laid out of the screen's regions that it shows in part: of each,
    /// its first line, a page, the last first line.
    Laid(ScreenLaid),
    /// The instant the application stands at now.
    Time(DateTime<Utc>),
    /// What a piece of work remarks while it works.
    Remark(Ticket, Remark),
    /// What lasting work hands back while it lasts.
    News(Ticket, News),
    /// What a piece of work came to.
    Outcome(Ticket, Outcome),
    /// A piece of what the performer's one following handed, under no ticket: a
    /// following is never let go, only followed anew.
    FollowNews(FollowNews),
    /// A look at what the performer's one following follows failed, its making among
    /// them, said in the words a failed piece of work is said in, under no ticket as its
    /// news; the following goes on, or is made again. Not `FollowNews::Unreadable`, one run
    /// the field cannot read.
    LookFailed(String),
    /// A page or a link the surface was asked to show and could not: the person opens it
    /// themselves.
    Unshown(Address),
}
