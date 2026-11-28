//! The blocks a screen that stays open is made of, for the catalog: what a
//! terminal, a page and a desktop draw at the top of a run, in its pane and at its foot.

use super::*;
use crate::components::{
    Attention, Card, CardBody, Checklist, ChoiceRow, Choices, Fields, Found, Horizon, Lead, Legend,
    Noted, Ribbon, SameAtAShell, Section, Stretch, Wordmark,
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
        key: None,
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
            .arrows("choose")
            .key("enter", "answer")
            .key("esc", "leave it waiting"),
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
        Attention::key("the plan waits for approval", "a", "go there").into(),
        Attention::key(
            "the suite passed at baseline and now exits 101",
            "a",
            "go there",
        )
        .of("Q78GRR")
        .into(),
        Attention::elsewhere("node `implement` asks at its terminal").into(),
        Attention::unkeyed(
            "node `grill` asks 3 questions: nothing is recorded until every one is answered",
        )
        .of("Q78GRR")
        .into(),
        Noted {
            mark: Mark::Done,
            said: "Recorded: approve. On to `pr`.".to_string(),
        }
        .into(),
        card().into(),
        crate::components::desk::tests::plan().into(),
        crate::components::typed_answer::tests::requested().into(),
        crate::components::word_choice::tests::modes().into(),
        crate::components::form::tests::new_run().into(),
        crate::components::survey::tests::a_role_changed().into(),
        installing_at_a_shell().into(),
        crate::components::choice_list::tests::shapes().into(),
        crate::components::key_list::tests::help().into(),
        moved_through(true).into(),
        moved_through(false).into(),
        installed().into(),
        crate::components::finding_list::tests::answered().into(),
        crate::components::feed::tests::scrolled_back().into(),
        crate::components::meter::tests::waited_on_you().into(),
        crate::components::at_a_glance::tests::asking().into(),
        Wordmark.into(),
        Horizon { tone: Tone::Done }.into(),
    ]
}

/// What installing a pack and setting a repository up do at a shell, as a confirmation names
/// them: the first command wider than the narrowest row.
fn installing_at_a_shell() -> SameAtAShell {
    SameAtAShell(vec![
        "yunta pack add https://github.com/eulke/yunta//packs/fragua@v0.0.5".to_string(),
        "yunta init".to_string(),
    ])
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
