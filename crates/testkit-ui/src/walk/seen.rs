//! See [`super`]. How the walk tells a state apart ([`Seen`]): by all it shows and holds, or,
//! where a palette or help stands over it, by that box alone, which the walk keeps once and
//! walks no further.

use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use surco::components::Screen;
use surco::surface::{Markdown, Surface};
use yunta_core::RunId;
use yunta_ui::{Dialog, State};

use super::classes::stage_kind;

/// How the walk tells a state apart: a fingerprint of its screen as Markdown draws it, the
/// kinds of work it waits on, the rows its regions are scrolled to, where a run's pane
/// stands, the stages it has yet to open, what it follows, how deep the field the person is
/// at lists its runs and which runs it holds; and that Markdown, which the checks read.
pub(super) struct Seen {
    pub(super) fingerprint: u64,
    pub(super) markdown: String,
}

/// What the fingerprint of a state with a palette or help over it begins with, so it is never
/// taken for another state's: a word no screen hashes first.
const BOXED: &str = "a box over the screen";

impl Seen {
    /// How the walk tells `state` apart: a palette or help standing over it, by that box
    /// ([`boxed`]); any other state by all it shows and holds ([`shown`]).
    pub(super) fn of(state: &State) -> Seen {
        let screen = state.screen(&Markdown.look());
        let markdown = Markdown.screen(&screen);
        let fingerprint = match opens_a_box(state) {
            true => boxed(state, &screen),
            false => shown(state, &screen, &markdown),
        };
        Seen {
            fingerprint,
            markdown,
        }
    }
}

/// The fingerprint of `state`, whose screen is `screen` with a palette or help standing over
/// it: that box, the stage under it and which of the rows a box stands among the screen holds.
/// A box lays out by what it holds and the rows around it, the meter's and the one over the
/// legend: what it covers is told apart no further.
fn boxed(state: &State, screen: &Screen) -> u64 {
    let drawn = screen
        .overlay
        .as_ref()
        .map(|overlay| (&overlay.title, Markdown.draw(&overlay.doc)));
    let around = (
        screen.meter.is_some(),
        screen.noted.is_some(),
        screen.attention.is_some(),
    );
    let mut fingerprint = DefaultHasher::new();
    (BOXED, drawn, around, state.stage().map(stage_kind)).hash(&mut fingerprint);
    fingerprint.finish()
}

/// The fingerprint of `state`, whose screen is `screen`, drawn as `markdown`: all it shows and
/// holds, as [`Seen`] says.
fn shown(state: &State, screen: &Screen, markdown: &str) -> u64 {
    // What it waits on, by kind: the walk answers every piece of work of one kind
    // with the same recorded outcomes, whichever run it names.
    let kinds: BTreeSet<u64> = state
        .waiting_on()
        .iter()
        .map(|(_, work)| {
            let mut kind = DefaultHasher::new();
            work.kind().hash(&mut kind);
            kind.finish()
        })
        .collect();
    // Where each region is scrolled to, which Markdown, drawing every line, does not say.
    let tops = (
        screen.main.sight.top,
        screen.side.as_ref().map(|side| side.region.sight.top),
    );
    // Where a run's pane or the page of its numbers stands, and whether the last layout
    // left it anything to scroll, which no row of either screen says: a state a surface
    // laid out is told from the one it left, so the keys that scroll are tried where they
    // move something.
    let pane = state.pane().map(|pane| (pane.to(), pane.scrolls()));
    // The stages a step opened the application on, which no screen shows before it
    // first stands somewhere: each opening's own. A stage keeps no hash of its own, so
    // they are told apart as they say themselves, and only where there are any.
    let stages = state.stages_to_open();
    let to_open = (!stages.is_empty()).then(|| format!("{stages:?}"));
    // What it follows, which the news it is handed are chosen by: the field's reach
    // among it.
    let followed = state.followed();
    // How deep the field lists its runs, which its screen does not say while no closed run
    // stands in it, and which decides what `d` does and whether a run that closes later
    // shows.
    let depth = state.field_depth();
    let held = held(state);
    let mut fingerprint = DefaultHasher::new();
    (markdown, kinds, tops, pane, to_open, followed, depth, held).hash(&mut fingerprint);
    fingerprint.finish()
}

/// Whether a palette or help stands over `state`: a box the walk keeps once, and walks no
/// further.
pub(super) fn opens_a_box(state: &State) -> bool {
    matches!(state.dialog(), Some(Dialog::Palette(_) | Dialog::Help(_)))
}

/// The runs the field the person is at holds, and how many it leaves to other repositories,
/// which its screen does not say at a glance (a closed run is listed only with `d`, and a run
/// elsewhere only at the machine's reach), and which decide what those show; none away from
/// the field.
fn held(state: &State) -> Option<(Vec<&RunId>, usize)> {
    let field = state
        .field_depth()
        .and(state.followed_runs().field.as_ref())?;
    Some((field.runs.keys().collect(), field.elsewhere))
}
