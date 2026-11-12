//! The dialogs the walk opens over each state, apart from the strokes it walks: the
//! palette, every row of each one it has not met taken; help, opened over each legend it
//! has not met, listing each of its keys and closed with nothing done; and the rule that
//! no work which stops, closes, removes or spends is asked but by Enter on its
//! confirmation.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{catch_unwind, AssertUnwindSafe};

use surco::doc::Block;
use surco::surface::{Markdown, Surface};
use yunta_testkit_ui::walk::Reached;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use super::InterfaceChecks;

/// What palettes the walk must meet offer, by the command a row names, and where it
/// offers it: one row of every stage that offers acts of its own, and those every stage
/// offers.
pub const MUST_OFFER: [(&str, &str); 11] = [
    ("yunta resume", "the field"),
    ("yunta resolve-gate", "a parked run"),
    ("yunta stats", "a run"),
    ("yunta open", "a run's page for a browser"),
    ("yunta check", "the workflows"),
    ("yunta pack audit", "the packs"),
    ("yunta doctor --session", "doctor"),
    ("yunta gc", "upkeep"),
    ("yunta list", "every stage"),
    ("yunta doctor", "every stage"),
    ("yunta schema", "every stage"),
];

impl InterfaceChecks<'_> {
    /// The palette over `state`, reached as `reached` says, opened: where it is one the walk
    /// has not met, each of its rows taken, which holds as every input does.
    pub fn palette_taken(&mut self, state: &State, reached: &Reached<'_>) {
        let mut opened = state.clone();
        let opening = reached.then("Stroke(Insert('/'))");
        let asked = catch_unwind(AssertUnwindSafe(|| {
            opened.update(Input::Stroke(Stroke::Insert('/')))
        }))
        .unwrap_or_else(|_| panic!("opening the palette panicked, after:\n{}", opening.inputs()));
        self.recordings.carried_out(&asked, &opening);
        let rows = palette_rows(&opened);
        let mut fingerprint = DefaultHasher::new();
        rows.hash(&mut fingerprint);
        if rows.is_empty() || !self.palettes.insert(fingerprint.finish()) {
            return;
        }
        self.unoffered
            .retain(|(command, _)| !rows.iter().any(|row| row.starts_with(command)));
        for (row, named) in rows.iter().enumerate() {
            let taking = opening.then(format!("the row {named:?}"));
            let mut chosen = opened.clone();
            let asked = catch_unwind(AssertUnwindSafe(|| {
                for _ in 0..row {
                    chosen.update(Input::Stroke(Stroke::Down));
                }
                chosen.update(Input::Stroke(Stroke::Enter))
            }))
            .unwrap_or_else(|_| panic!("a palette's row panicked, after:\n{}", taking.inputs()));
            self.recordings.carried_out(&asked, &taking);
            let enter = Input::Stroke(Stroke::Enter);
            nothing_unconfirmed(&asked, &opened, &enter, &taking);
        }
    }

    /// Help over `state`, reached as `reached` says, opened where its legend offers it and
    /// is one the walk has not met: it asks nothing, lists every key of the legend, and
    /// Escape closes it, leaving the screen as it was.
    pub fn help_taken(&mut self, state: &State, reached: &Reached<'_>) {
        let screen = state.screen(&Markdown.look());
        let keys: Vec<(String, String)> = screen
            .legend
            .0
            .iter()
            .map(|binding| (binding.keys.named().to_string(), binding.does.clone()))
            .collect();
        if !keys.iter().any(|(key, _)| key == "?") {
            return;
        }
        let mut fingerprint = DefaultHasher::new();
        keys.hash(&mut fingerprint);
        if !self.helps.insert(fingerprint.finish()) {
            return;
        }
        let opening = reached.then("Stroke(Insert('?'))");
        let mut opened = state.clone();
        let asked = opened.update(Input::Stroke(Stroke::Insert('?')));
        assert!(
            asked.is_empty(),
            "help asked {asked:?}, after:\n{}",
            opening.inputs()
        );
        let help = opened
            .screen(&Markdown.look())
            .overlay
            .filter(|overlay| overlay.title == yunta_render::screen::HELP)
            .unwrap_or_else(|| panic!("no help opened, after:\n{}", opening.inputs()));
        let listed = Markdown.draw(&help.doc);
        for (_, does) in &keys {
            assert!(
                listed.contains(does.as_str()),
                "help leaves out {does:?}, after:\n{}",
                opening.inputs()
            );
        }
        let asked = opened.update(Input::Stroke(Stroke::Decline));
        assert!(asked.is_empty(), "closing help asked {asked:?}");
        let closed = Markdown.screen(&opened.screen(&Markdown.look()));
        assert_eq!(
            closed,
            Markdown.screen(&screen),
            "help closed leaves the screen as it was"
        );
    }
}

/// The rows of the palette standing over `state`, each the command it names; none where
/// no palette stands.
fn palette_rows(state: &State) -> Vec<String> {
    let screen = state.screen(&Markdown.look());
    let Some(overlay) = screen
        .overlay
        .filter(|overlay| overlay.title == yunta_render::screen::PALETTE)
    else {
        return Vec::new();
    };
    overlay
        .doc
        .blocks()
        .iter()
        .find_map(|block| match block {
            Block::ChoiceList(list) => list.choices.as_ref(),
            _ => None,
        })
        .map(|choices| choices.rows.iter().map(|row| row.name.clone()).collect())
        .unwrap_or_default()
}

/// `asked`, what `input` asked of `before`, holds no work that stops, closes, removes or
/// spends but where the input is Enter on a confirmation standing over `before`; `reached`
/// says the inputs that asked where it does.
pub fn nothing_unconfirmed(
    asked: &[Request],
    before: &State,
    input: &Input,
    reached: &Reached<'_>,
) {
    let unconfirmed = asked.iter().find_map(|request| match request {
        Request::Perform(_, operation) if operation.needs_confirming() => Some(operation),
        _ => None,
    });
    let Some(operation) = unconfirmed else {
        return;
    };
    let confirming = before
        .screen(&Markdown.look())
        .overlay
        .is_some_and(|overlay| overlay.title == yunta_render::screen::CONFIRM);
    assert!(
        confirming && matches!(input, Input::Stroke(Stroke::Enter)),
        "{operation:?} asked with no confirmation, after:\n{}",
        reached.inputs()
    );
}
