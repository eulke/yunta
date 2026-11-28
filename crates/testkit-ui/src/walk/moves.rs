//! See [`super`]. The moves the walk makes on a state ([`Move`]): an input handed to it, or a
//! row of the palette over it taken as a person takes it, the palette opened, the pointer moved
//! down to the row and Enter. Each row of each palette the walk meets is taken over the first
//! state it meets it over, once the opening is walked whole (`palettes_taken`).

use std::hash::{DefaultHasher, Hash, Hasher};

use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Request, State};

use super::checking::{HandedOn, Kept, Unchecked};
use super::interface::dialogs::palette_rows;
use super::{filtered, Came, Checks, Handed, Walk, STROKED_WHILE_WAITING};

/// A move the walk makes on a state: an input handed to it, or the row of the palette over it
/// at `row` taken, `named` by what it does, as a person takes it: the palette opened, the
/// pointer moved down to the row, and Enter; the palette told apart by its rows (`palette`).
pub(super) enum Move {
    Input(Input),
    PaletteRow {
        row: usize,
        named: String,
        palette: u64,
    },
}

/// The key that opens the palette over a state where nothing is typed.
pub(super) const OPENS_THE_PALETTE: Stroke = Stroke::Insert('/');

/// The key that opens help over a state where nothing is typed.
pub(super) const OPENS_HELP: Stroke = Stroke::Insert('?');

impl Move {
    /// The move made on `state`: what it asked.
    pub(super) fn made(&self, state: &mut State) -> Vec<Request> {
        match self {
            Move::Input(input) => state.update(input.clone()),
            Move::PaletteRow { row, .. } => {
                let mut asked = state.update(Input::Stroke(OPENS_THE_PALETTE));
                for _ in 0..*row {
                    asked.extend(state.update(Input::Stroke(Stroke::Down)));
                }
                asked.extend(state.update(Input::Stroke(Stroke::Enter)));
                asked
            }
        }
    }

    /// The move as the inputs that reach a state name it, one a line.
    pub(super) fn named(&self) -> String {
        match self {
            Move::Input(input) => format!("{input:?}"),
            Move::PaletteRow { named, .. } => {
                format!("Stroke({OPENS_THE_PALETTE:?})\nthe palette's row {named:?}")
            }
        }
    }
}

impl Walk<'_> {
    /// Every move the walk makes on `state`: each input it can be handed, and, where the
    /// strokes are tried and nothing is typed, each row of the palette over it taken, where
    /// the walk has not met that palette yet.
    pub(super) fn moves(&self, state: &State) -> Vec<Move> {
        let inputs = self.inputs(state).into_iter().map(Move::Input);
        let stroked = state.waiting_on().len() <= STROKED_WHILE_WAITING;
        let typing = state.typing() || filtered(state);
        let (rows, palette) = match stroked && !typing {
            true => palette_rows_over(state),
            false => (Vec::new(), 0),
        };
        let unmet = !self.palettes.contains(&palette);
        let rows = rows
            .into_iter()
            .enumerate()
            .filter(|_| unmet)
            .map(|(row, named)| Move::PaletteRow {
                row,
                named,
                palette,
            });
        inputs.chain(rows).collect()
    }

    /// The moves made on `state`, reached at `at`, split: the rows of the palette over it,
    /// kept in `met` for once the opening is walked whole where the walk meets that palette
    /// first over it, and every other move, which the walk takes at once.
    pub(super) fn rows_kept(
        &mut self,
        met: &mut Vec<(usize, State, Vec<Handed>)>,
        (at, state): (usize, &State),
        handed: Vec<Handed>,
    ) -> Vec<Handed> {
        let (rows, handed): (Vec<Handed>, Vec<Handed>) = (handed.into_iter())
            .partition(|handed| matches!(handed.moved, Move::PaletteRow { .. }));
        if let Some(Move::PaletteRow { palette, .. }) = rows.first().map(|row| &row.moved) {
            if self.palettes.insert(*palette) {
                met.push((at, state.clone(), rows));
            }
        }
        handed
    }

    /// Each row of each palette in `met`, taken over the first state the walk met it over, once
    /// the opening is walked whole, so no state a row leads to stands for one another way
    /// reaches: where the state a row leads to is one no other way reached, it is kept and
    /// every one of `checks` holds it, and it is walked no further.
    pub(super) fn palettes_taken(
        &mut self,
        met: Vec<(usize, State, Vec<Handed>)>,
        checks: &mut [&mut dyn Checks],
    ) {
        let mut unchecked = Unchecked::default();
        for (to, (at, state, rows)) in met.into_iter().enumerate() {
            for Handed { moved, came } in rows {
                let reached = self.at(at, &moved);
                let Some(Came { asked, seen, next }) = came else {
                    panic!(
                        "a palette's row panicked, reached by:\n{}",
                        reached.inputs()
                    )
                };
                self.recordings.carried_out(&asked, &reached);
                let kept = next.and_then(|next| {
                    let named = || moved.named();
                    let place = self.kept(seen.fingerprint, &next, Some(at), named)?;
                    Some(Kept {
                        place,
                        state: next,
                        markdown: seen.markdown,
                    })
                });
                unchecked.handed.push(HandedOn {
                    to,
                    moved,
                    asked,
                    kept,
                });
            }
            unchecked.expanded.push((at, state));
        }
        self.checked(&unchecked, checks);
    }
}

/// What each row of the palette over `state` does, in the order it lists them, and the
/// palette told apart by its rows: none where no palette opens over it.
fn palette_rows_over(state: &State) -> (Vec<String>, u64) {
    let mut opened = state.clone();
    opened.update(Input::Stroke(OPENS_THE_PALETTE));
    let rows = palette_rows(&opened);
    let mut palette = DefaultHasher::new();
    rows.hash(&mut palette);
    let named = rows.into_iter().map(|(named, _)| named).collect();
    (named, palette.finish())
}
