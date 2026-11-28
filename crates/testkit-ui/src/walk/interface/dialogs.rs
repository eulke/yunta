//! The dialogs the walk opens over each state, apart from the strokes it walks: the
//! palette, every row of each one it has not met taken, a row that names a key doing what
//! pressing that key does; help, opened over each legend it has not met, listing each of
//! its keys, saying with the legend every key the state takes, and closed with nothing done;
//! and the rule that no work which stops, closes, removes or spends is asked but by Enter on
//! its confirmation.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{catch_unwind, AssertUnwindSafe};

use surco::components::Screen;
use surco::doc::Block;
use surco::surface::Markdown;
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Operation, Request, State};

use super::InterfaceChecks;
use crate::walk::classes::stage_kind;
use crate::walk::strokes::strokes;
use crate::walk::Reached;

/// What palettes the walk must meet offer, by the command a row names, and where it
/// offers it: one row of every stage that offers acts of its own, and those every stage
/// offers.
pub(super) const MUST_OFFER: [(&str, &str); 13] = [
    ("yunta resume", "the field"),
    ("yunta run", "the field"),
    ("yunta pack add", "the audit of the pack a setup installs"),
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
    /// has not met, each of its rows taken, which holds as every input does, and a row that
    /// names a key asks what pressing that key on `state` asks and leaves the same screen.
    pub(super) fn palette_taken(&mut self, state: &State, reached: &Reached<'_>) {
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
            .retain(|(command, _)| !rows.iter().any(|(named, _)| named.starts_with(command)));
        for (row, (named, key)) in rows.iter().enumerate() {
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
            if let Some(key) = key {
                does_what_its_key_does(state, key, (&chosen, &asked), &taking);
            }
        }
    }

    /// Help over `state`, whose screen as Markdown lays it out is `screen`, reached as
    /// `reached` says, opened where its legend or the far keys beside it offer it and the
    /// legend is one the walk has not met, at the field for each depth and reach it is asked to
    /// show, which its one legend does not say and help over it does, and for each stage, page
    /// that scrolls, place a run can or cannot be asked for and attention row standing or not:
    /// it asks nothing, lists every key the legend names, alone or beside another on one of its
    /// rows (`s g`), says with the legend and the far keys every key that changes the state
    /// (`every_key_it_takes_said`), and Escape closes it, leaving the screen as it was.
    pub(super) fn help_taken(&mut self, state: &State, screen: &Screen, reached: &Reached<'_>) {
        let keys: Vec<(String, String)> = screen
            .legend
            .0
            .iter()
            .map(|binding| (binding.keys.named().to_string(), binding.does.clone()))
            .collect();
        let far = screen.anywhere.0.iter().map(|binding| binding.keys.named());
        let mut offered = far.chain(keys.iter().map(|(key, _)| key.as_str()));
        if !offered.any(|key| key == "?") {
            return;
        }
        if !self.helps.insert(help_told_apart(state, screen, &keys)) {
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
        let listed = listed_keys(&help.doc);
        for (key, does) in &keys {
            let said = key
                .split_whitespace()
                .all(|key| listed.iter().any(|listed| listed == key));
            assert!(
                said,
                "help leaves out {key:?}, which the legend says {does:?}, after:\n{}",
                opening.inputs()
            );
        }
        every_key_it_takes_said(state, screen, (&keys, &listed), reached);
        let asked = opened.update(Input::Stroke(Stroke::Decline));
        assert!(asked.is_empty(), "closing help asked {asked:?}");
        let closed = Markdown.screen(&opened.screen(&Markdown.look()));
        assert_eq!(
            closed,
            Markdown.screen(screen),
            "help closed leaves the screen as it was"
        );
    }
}

/// How the walk tells apart help over `state`, whose legend names `keys`: by those keys, the
/// stage it lists them over, whether that stage's page scrolls, whether a run can be asked for
/// where it stands and whether its attention row names anything, which decide which keys it
/// takes beyond its legend; and at the field by what it is asked to show, its depth and its
/// reach, which decide what `d` and `e` do there and which its one legend does not say.
fn help_told_apart(state: &State, screen: &Screen, keys: &[(String, String)]) -> u64 {
    let field = state
        .field_depth()
        .map(|depth| (depth, state.followed().field));
    let stage = state.stage().map(stage_kind);
    let scrolls = state.pane().map(|pane| pane.scrolls());
    let asks_for_a_run = state.followed_runs().can_ask_for_a_run();
    let attention = screen.attention.is_some();
    let mut fingerprint = DefaultHasher::new();
    (keys, field, stage, scrolls, asks_for_a_run, attention).hash(&mut fingerprint);
    fingerprint.finish()
}

/// Every key the walk presses that changes `state`, whose screen as Markdown lays it out is
/// `screen`, reached as `reached` says, is among the keys said by its legend (`keys`), the far
/// keys beside it and help over it (`listed`): a key changes the state where pressing it asks
/// for anything or leaves another screen. The keys that move what is read go together, as
/// help says them: the arrows where `j k`, the arrows or the page keys are said, the page
/// keys, `home`, `end` and the space bar where the arrows or the page keys are.
fn every_key_it_takes_said(
    state: &State,
    screen: &Screen,
    (keys, listed): (&[(String, String)], &[String]),
    reached: &Reached<'_>,
) {
    let far = screen.anywhere.0.iter().map(|binding| binding.keys.named());
    let legend = keys.iter().map(|(key, _)| key.as_str()).chain(far);
    let said: Vec<&str> = legend
        .flat_map(str::split_whitespace)
        .chain(listed.iter().map(String::as_str))
        .collect();
    let drawn = Markdown.screen(screen);
    let is_said = |key: &str| said.contains(&key);
    let scroll_said = is_said("up-down") || is_said("pgup") || is_said("pgdn");
    for stroke in strokes() {
        let key = spelled(stroke);
        let in_its_family = match stroke {
            Stroke::Up | Stroke::Down => scroll_said || is_said("j") || is_said("k"),
            Stroke::PageUp | Stroke::PageDown | Stroke::Home | Stroke::End => scroll_said,
            Stroke::Insert(' ') => scroll_said,
            _ => false,
        };
        if is_said(&key) || in_its_family {
            continue;
        }
        let mut pressed = state.clone();
        let asked = pressed.update(Input::Stroke(stroke));
        let changed =
            !asked.is_empty() || Markdown.screen(&pressed.screen(&Markdown.look())) != drawn;
        assert!(
            !changed,
            "`{key}` changes the state and neither its legend nor help says it, after:\n{}",
            reached.then(format!("Stroke({stroke:?})")).inputs()
        );
    }
}

/// `stroke` as a legend or help names its key: a character as it is typed, the space bar as
/// `space`, every other key by its name.
fn spelled(stroke: Stroke) -> String {
    match stroke {
        Stroke::Insert(' ') => "space".to_string(),
        Stroke::Insert(character) => character.to_string(),
        Stroke::Enter => "enter".to_string(),
        Stroke::Decline => "esc".to_string(),
        Stroke::Interrupt => "ctrl-c".to_string(),
        Stroke::Tab => "tab".to_string(),
        Stroke::BackTab => "shift-tab".to_string(),
        Stroke::Up | Stroke::Down => "up-down".to_string(),
        Stroke::Left | Stroke::Right => "left-right".to_string(),
        Stroke::PageUp => "pgup".to_string(),
        Stroke::PageDown => "pgdn".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Each key help's list `doc` names on its rows, one by one: `s` and `g` for its row `s g`.
fn listed_keys(doc: &surco::doc::Doc) -> Vec<String> {
    let listed = doc.blocks().iter().find_map(|block| match block {
        Block::KeyList(list) => Some(&list.keys.0),
        _ => None,
    });
    listed
        .into_iter()
        .flatten()
        .flat_map(|binding| binding.keys.named().split_whitespace())
        .map(str::to_string)
        .collect()
}

/// `chosen`, the palette's row naming `key` taken over `state`, asked what `asked` holds:
/// the same work pressing `key` on `state` asks, and the same screen after it, as Markdown
/// draws it; `taking` says the inputs that reach the row.
fn does_what_its_key_does(
    state: &State,
    key: &str,
    (chosen, asked): (&State, &[Request]),
    taking: &Reached<'_>,
) {
    let stroke = match key {
        "enter" => Stroke::Enter,
        typed => Stroke::Insert(typed.chars().next().expect("a key is spelled")),
    };
    let mut pressed = state.clone();
    let by_the_key = pressed.update(Input::Stroke(stroke));
    let work = |asked: &[Request]| -> Vec<Operation> {
        asked
            .iter()
            .filter_map(|request| match request {
                Request::Perform(_, operation) => Some(operation.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        work(asked),
        work(&by_the_key),
        "the row naming `{key}` asks what the key does not, after:\n{}",
        taking.inputs()
    );
    let drawn = |state: &State| Markdown.screen(&state.screen(&Markdown.look()));
    assert_eq!(
        drawn(chosen),
        drawn(&pressed),
        "the row naming `{key}` leaves another screen than the key, after:\n{}",
        taking.inputs()
    );
}

/// The rows of the palette standing over `state`, each the command that does the same and
/// the key its stage takes it by, where one does; none where no palette stands.
pub(crate) fn palette_rows(state: &State) -> Vec<(String, Option<String>)> {
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
        .map(|choices| {
            choices
                .rows
                .iter()
                .filter_map(|row| Some((row.said.clone()?, row.key.clone())))
                .collect()
        })
        .unwrap_or_default()
}

/// `asked`, what `input` asked of `before`, holds no work that stops, closes, removes or
/// spends but where the input is Enter on a confirmation standing over `before`; `reached`
/// says the inputs that asked where it does.
pub(super) fn nothing_unconfirmed(
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
        .is_some_and(|overlay| overlay.confirms);
    assert!(
        confirming && matches!(input, Input::Stroke(Stroke::Enter)),
        "{operation:?} asked with no confirmation, after:\n{}",
        reached.inputs()
    );
}
