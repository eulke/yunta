//! See [`crate`]. What reaches the application.

use chrono::{DateTime, Utc};
use surco::components::ScreenLaid;
use yunta_api::Remark;

use crate::prompt::Stroke;
use crate::{News, Outcome, Ticket};

/// What reaches the application: a stroke a person gave, what a surface laid out, the
/// time, and what work it asked for says while it works, hands back while it lasts and
/// came to.
#[derive(Debug)]
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
}
