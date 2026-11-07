//! The blocks a screen that stays open is made of, for the catalog: what a
//! terminal, a page and a desktop draw at the top of a run and at its foot.

use super::*;
use crate::components::{Attention, Legend, Ribbon, Stretch};

/// A run's ribbon: two nodes finished, the one running with its plow, and the
/// rest still ahead.
fn ribbon() -> Ribbon {
    let ahead = |name: &str| Stretch::new(name, Standing::NeverRan);
    Ribbon::new([
        Stretch::new("grill", Standing::Finished),
        Stretch::new("brief", Standing::Finished),
        Stretch::new("plan", Standing::Running),
        ahead("build"),
        ahead("review"),
        ahead("lint"),
        ahead("ship"),
    ])
    .plowed(5)
}

/// The keys of a field of runs: the arrows among them.
fn legend() -> Legend {
    Legend::new()
        .arrows("move")
        .key("enter", "open")
        .key("n", "new run")
        .key("x", "close")
        .key("q", "quit")
}

/// The blocks of a screen, under their heading: the part the catalog appends
/// after its last one.
pub(super) fn on_a_screen() -> Vec<Block> {
    vec![
        Block::Heading("on a screen".to_string()),
        ribbon().into(),
        legend().into(),
        Attention::command(
            "the plan waits for approval",
            "yunta resolve-gate 7E5PH4 approve",
        )
        .into(),
        Attention::key(
            "the suite passed at baseline and now exits 101",
            "a",
            "go there",
        )
        .of("Q78GRR")
        .into(),
        Attention::elsewhere("node `implement` asks at its terminal").into(),
    ]
}
