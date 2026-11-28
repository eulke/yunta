//! See [`super`]. The frame a page over a run stands in, the run's numbers, its graph, its
//! whole chronicle or what it leaves (its receipt, its evidence verified, its page for a
//! browser): the tally and the attention row as `FollowedRuns::framing` composes them
//! for every stage, the run's own decision out of sight under the page, so the row names it
//! where the run waits on the person; and the far keys beside the page's own where no line is
//! typed. `a` goes where the row names, by the rule every screen that draws it follows
//! (`FollowedRuns::attended`): on the run's own decision, back to the run, the lead given back
//! and its prompt in place; on another run, that run's screen in place of the page.

use chrono::{DateTime, Utc};
use surco::components::{Legend, Screen};
use yunta_core::RunId;

use super::Want;
use crate::followed_runs::{Attended, ItsDecision, Standpoint};
use crate::FollowedRuns;

/// `screen`, a page over `run`, in the run's frame as `followed` keeps it at `now`, a line
/// typed in what stands over it where `typed`.
pub(super) fn in_the_runs_frame(
    screen: &mut Screen,
    run: &RunId,
    followed: &FollowedRuns,
    now: DateTime<Utc>,
    typed: bool,
) {
    (screen.tally, screen.attention) = followed.framing(standing_over(run, typed), now);
    screen.anywhere = match typed {
        true => Legend::new(),
        false => yunta_render::screen::anywhere(),
    };
}

/// What `a` asks for on a page over `run`, as `followed` keeps it at `now`: on the run's own
/// decision, which its attention row names, back to the run and on to that decision; on
/// another run, that run's screen in place of the page; nothing where the row names none.
pub(super) fn attending(run: &RunId, followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Want> {
    match followed.attended(standing_over(run, false), now) {
        Some(Attended::ItsOwn) => vec![Want::BackToItsDecision],
        Some(Attended::Another(named)) => Want::in_place(&named),
        None => Vec::new(),
    }
}

/// Whether the attention row of a page over `run` names anything `a` goes to, as `followed`
/// keeps it at `now`: the run's own decision or another run, where the palette offers going
/// there.
pub(super) fn attends(run: &RunId, followed: &FollowedRuns, now: DateTime<Utc>) -> bool {
    followed.attended(standing_over(run, false), now).is_some()
}

/// Where the person stands on a page over `run`: the run's own decision out of sight under
/// it, a line typed there where `typed`.
fn standing_over(run: &RunId, typed: bool) -> Standpoint<'_> {
    Standpoint {
        run: Some((run, ItsDecision::OutOfSight)),
        typed,
    }
}
