//! See [`super`]. The states a walk kept, counted by class as it keeps them: the stage the
//! person is at, the stages under it, outermost first, and what stands over it, each by
//! kind. A walk's count says only that its states moved; its table, kept from one tree and
//! read beside the next's, says which classes moved, so a count that moves is explained by
//! comparing two tables instead of by walking again. The class is read, never told apart
//! by: what the walk keeps a state by is its own.
//!
//! A person asks for the table by naming a file in [`TABLE`] as they run the walk's test,
//! which writes it there ([`asked`]); unnamed, nothing is written.

use std::collections::BTreeMap;
use std::path::PathBuf;

use yunta_ui::{Dialog, Stage, State};

/// The variable a person names the file the walk's table is written to with: its totals on
/// the first line, then its states by class.
pub const TABLE: &str = "YUNTA_WALK_TABLE";

/// Where a person asked the walk's table written, by [`TABLE`]; none where they did not.
pub fn asked() -> Option<PathBuf> {
    std::env::var_os(TABLE).map(PathBuf::from)
}

/// The cells a class's count is right-aligned in on its line of the table: a count of the
/// walk's bound fits with a cell to spare.
const COUNT_CELLS: usize = 7;

/// What a walk reached: how many states it kept, and how many of them stand in each class,
/// by the line that names it. Not the states, which the walk keeps no more of than their
/// fingerprints.
#[derive(Debug, Default)]
pub struct Walked {
    pub(super) states: usize,
    classes: BTreeMap<String, usize>,
}

impl Walked {
    /// How many states the walk kept, which its classes' counts sum to.
    pub fn states(&self) -> usize {
        self.states
    }

    /// The table of the states by class: a line a class, `<count>  <class>`, sorted by
    /// class, a class read as `<stage> under=<stages> over=<dialog>`, `-` for none, the
    /// stages under it joined by `,`.
    pub fn by_class(&self) -> String {
        self.classes
            .iter()
            .map(|(class, count)| format!("{count:>COUNT_CELLS$}  {class}\n"))
            .collect()
    }

    /// `state`, just kept, counted in its class.
    pub(super) fn kept(&mut self, state: &State) {
        *self.classes.entry(class(state)).or_default() += 1;
    }
}

/// The class `state` stands in: the stage the person is at, the stages under it and what
/// stands over it, each by kind.
fn class(state: &State) -> String {
    let at = state.stage().map_or("-", stage_kind);
    let under: Vec<&str> = state.stages_under().map(stage_kind).collect();
    let under = match under.is_empty() {
        true => "-".to_string(),
        false => under.join(","),
    };
    let over = state.dialog().map_or("-", dialog_kind);
    format!("{at} under={under} over={over}")
}

/// The kind of `stage`, as its variant names it.
pub(crate) fn stage_kind(stage: &Stage) -> &'static str {
    match stage {
        Stage::Signpost => "Signpost",
        Stage::Field(_) => "Field",
        Stage::Run(_) => "Run",
        Stage::Chronicle(_) => "Chronicle",
        Stage::FirstRun(_) => "FirstRun",
        Stage::Start(_) => "Start",
        Stage::Setup(_) => "Setup",
        Stage::Catalog(_) => "Catalog",
        Stage::Evidence(_) => "Evidence",
        Stage::Stats(_) => "Stats",
        Stage::Graph(_) => "Graph",
        Stage::Doctor(_) => "Doctor",
        Stage::Upkeep(_) => "Upkeep",
        Stage::Schema(_) => "Schema",
    }
}

/// The kind of `dialog`, as its variant names it.
fn dialog_kind(dialog: &Dialog) -> &'static str {
    match dialog {
        Dialog::Confirmation(_) => "Confirmation",
        Dialog::Palette(_) => "Palette",
        Dialog::Help(_) => "Help",
    }
}
