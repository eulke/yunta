//! The blocks a screen that stays open is made of, for the catalog: what a
//! terminal, a page and a desktop draw at the top of a run and at its foot.

use super::*;
use crate::components::{
    Attention, Card, CardBody, Checklist, ChoiceRow, Choices, Fields, Found, Lead, Legend, Ribbon,
    Section, Stretch,
};

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
        moved_through(true).into(),
        moved_through(false).into(),
        installed().into(),
    ]
}

/// A list of checks a person moves through, the pointer on its second row.
fn installed() -> Checklist {
    let mut checks = Checklist::default().pointing_at("acme/review");
    checks.push(
        Found::Holds,
        "acme/lint",
        "v1.2.0 (3f9c2e1), as its lock records it",
    );
    checks.push(
        Found::Caution,
        "acme/review",
        "v0.4.0 (91ab7d0), modified: its files no longer match its lock",
    );
    checks
}

/// A section of a list a person moves through, the pointer `on` it or not.
fn moved_through(on: bool) -> Section {
    let (name, does) = match on {
        true => ("lint-fix", "a check, and an agent that fixes what it finds"),
        false => (
            "tasks",
            "a plan of tasks, and an agent that works through them",
        ),
    };
    Section {
        lead: Some(Lead::Pointer { on }),
        title: Line::new()
            .push(Tone::Strong, name)
            .plain(format!(": {does}")),
        blocks: vec![Fields::new()
            .push_if("run", format!("yunta run {name}"))
            .into()],
    }
}
