//! The blocks a screen that stays open is made of, for the catalog: what a
//! terminal, a page and a desktop draw at the top of a run and at its foot.

use super::*;
use crate::components::{Attention, Card, CardBody, ChoiceRow, Choices, Legend, Ribbon, Stretch};

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

/// A gate put to a person: two options, the first pointed at, with where it
/// leads and the keys that answer it.
fn card() -> Card {
    let option = |name: &str, said: &str| ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: None,
    };
    Card {
        question: "gate `ship` needs a decision".to_string(),
        edge: Some("approve KQERFD".to_string()),
        about: vec!["the change is reviewed and its findings settled".to_string()],
        body: CardBody::Choosing {
            choices: Choices {
                rows: vec![
                    option("approve", "On to `pr`: opens the pull request"),
                    option("abort", "Pause here: nothing runs"),
                ],
                at: 0,
            },
            then: Some("pr, then the run finishes".to_string()),
        },
        legend: Legend::new()
            .arrows("move")
            .key("enter", "chooses")
            .key("esc", "leaves the run waiting"),
    }
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
        card().into(),
        crate::components::desk::tests::plan().into(),
        crate::components::typed_answer::tests::named().into(),
        crate::components::choice_list::tests::shapes().into(),
    ]
}
