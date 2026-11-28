//! See [`super`]. The frame a page about no one run stands in, the machine, upkeep, the
//! workflows and every page opened from them (a workflow's check, its past runs and its
//! graph, the cases, a new workflow, the packs, the documents' shapes): the tally and the
//! attention row as `FollowedRuns::framing` composes them for every stage, read from the field
//! the state follows under the page, the one it was opened over; and the far keys beside the
//! page's own where no line is typed. `a` goes where the row names, by the rule every screen that draws
//! it follows (`FollowedRuns::attended`): that run's screen, in the page's place; from a setup,
//! which stands at the bottom where the field would, opened over it as the field opens it,
//! since nothing drops the stage at the bottom; where the page takes `a` itself, as the
//! packs' listing takes it for an audit, the row names no key. Not `over_the_run`, the frame of a page
//! over one run, whose own decision the row names first.

use chrono::{DateTime, Utc};
use surco::components::{Legend, Screen};
use yunta_core::step::NextStep;

use super::Want;
use crate::followed_runs::{Attended, Standpoint};
use crate::FollowedRuns;

/// What `a` does on a page about no one run: it goes where the attention row names, or it is
/// the page's own, as the packs' listing takes it for an audit, and the row names no key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyA {
    Attends,
    TheStagesOwn,
}

/// Where the person stands on a page about no one run, a line typed there where `typed`.
fn standing(typed: bool) -> Standpoint<'static> {
    Standpoint { run: None, typed }
}

/// `screen`, a page about no one run, in the frame of the field `followed` keeps at `now`, a
/// line typed in what stands over it where `typed`; its attention row naming `a` where `a`
/// goes there.
pub(super) fn in_the_fields_frame(
    screen: &mut Screen,
    followed: &FollowedRuns,
    now: DateTime<Utc>,
    typed: bool,
    a: KeyA,
) {
    let unkeyed = typed || a == KeyA::TheStagesOwn;
    (screen.tally, screen.attention) = followed.framing(standing(unkeyed), now);
    screen.anywhere = match typed {
        true => Legend::new(),
        false => yunta_render::screen::anywhere(),
    };
}

/// Where `a` takes the person from a page about no one run, over what `followed` keeps at
/// `now`: to the run the attention row names, in the page's place; nowhere where it names
/// none.
pub(super) fn attending_over_the_field(followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Want> {
    match followed.attended(standing(false), now) {
        Some(Attended::Another(run)) => Want::in_place(&run),
        Some(Attended::ItsOwn) | None => Vec::new(),
    }
}

/// Whether the attention row of a page about no one run names a run `a` goes to, over what
/// `followed` keeps at `now`: where the palette offers going there.
pub(super) fn attends(followed: &FollowedRuns, now: DateTime<Utc>) -> bool {
    matches!(
        followed.attended(standing(false), now),
        Some(Attended::Another(_))
    )
}

/// Where `a` takes the person from a setup, which stands at the bottom of what stands where
/// the field would, over what `followed` keeps at `now`: to the run the attention row names,
/// its screen opened over the setup as the field opens it, so the setup stays under it;
/// nowhere where it names none.
pub(super) fn attending_from_a_setup(followed: &FollowedRuns, now: DateTime<Utc>) -> Vec<Want> {
    match followed.attended(standing(false), now) {
        Some(Attended::Another(run)) => Want::taking(&NextStep::status(&run)),
        Some(Attended::ItsOwn) | None => Vec::new(),
    }
}
