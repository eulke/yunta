//! A card is a block every surface draws: whole in a document, as Markdown and as
//! a page, in each of the three things it holds; and on a terminal, in every
//! environment, as a prompt holds it with all the rows it needs.

use std::path::{Path, PathBuf};

use super::{Card, CardBody, ChoiceRow, Choices, RoundRow, RoundStanding, TypedRow};
use crate::components::Legend;
use crate::doc::Doc;
use crate::golden::{assert_golden, ENVIRONMENTS, RICH};
use crate::look::Look;
use crate::surface::{Html, Markdown, Surface, Terminal};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/card")
}

fn option(name: &str, said: &str, cost: Option<&str>) -> ChoiceRow {
    ChoiceRow {
        name: name.to_string(),
        said: Some(said.to_string()),
        cost: cost.map(str::to_string),
    }
}

fn card(body: CardBody, legend: Legend) -> Card {
    Card {
        question: "plan `T001` needs a decision".to_string(),
        edge: Some("approve-plan · A32BZY".to_string()),
        about: vec!["the tasks document the planner handed over".to_string()],
        body,
        legend,
    }
}

/// Three options, the second pointed at, with where it leads.
fn choosing() -> Card {
    card(
        CardBody::Choosing {
            choices: Choices {
                rows: vec![
                    option("approve", "On to `implement`", None),
                    option(
                        "adjust",
                        "Back to `plan`: sends what you say",
                        Some("one more planning session"),
                    ),
                    option("abort", "Pause here: nothing runs", None),
                ],
                at: 1,
            },
            then: Some("plan, then this gate asks again".to_string()),
        },
        Legend::new()
            .arrows("move")
            .key("enter", "chooses")
            .key("esc", "leaves the run waiting"),
    )
}

/// The second option chosen, its words half typed and refused once.
fn saying() -> Card {
    card(
        CardBody::Saying {
            chosen: option("adjust", "Back to `plan`: sends what you say", None),
            asks: "what should the plan change".to_string(),
            typed: TypedRow {
                prompt: "> ".to_string(),
                before: "split the second task".to_string(),
                after: " in two".to_string(),
            },
            refused: Some("this option needs an answer".to_string()),
        },
        Legend::new()
            .key("enter", "sends it")
            .key("esc", "goes back"),
    )
}

/// A round of three: one answered, one typed into, one to come.
fn round() -> Card {
    card(
        CardBody::Round(vec![
            RoundRow {
                asked: "What changed?".to_string(),
                note: None,
                stands: RoundStanding::Answered("one thing".to_string()),
            },
            RoundRow {
                asked: "Any risk?".to_string(),
                note: Some("optional, assumes none".to_string()),
                stands: RoundStanding::Typing {
                    typed: TypedRow {
                        prompt: "> ".to_string(),
                        before: "the cache".to_string(),
                        after: String::new(),
                    },
                    refused: vec!["say what it risks".to_string()],
                },
            },
            RoundRow {
                asked: "How deep?".to_string(),
                note: None,
                stands: RoundStanding::Ahead,
            },
        ]),
        Legend::new()
            .key("enter", "answers")
            .key("esc", "leaves it for later"),
    )
}

/// Every card, by the name its goldens carry.
fn cards() -> [(&'static str, Card); 3] {
    [
        ("choosing", choosing()),
        ("saying", saying()),
        ("round", round()),
    ]
}

#[test]
fn a_card_is_written_whole_as_markdown_and_drawn_whole_on_a_page() {
    for (name, card) in cards() {
        let doc = Doc::new().with(card);
        assert_golden(&goldens().join(format!("{name}.md")), &Markdown.draw(&doc));
        assert_golden(&goldens().join(format!("{name}.html")), &Html.body(&doc));
    }
}

/// The two bodies the terminal's own golden leaves out, in a document, in every
/// environment: a document gives a card every row it needs.
#[test]
fn a_card_in_a_document_matches_its_goldens_in_every_environment() {
    for (name, card) in cards().into_iter().filter(|(name, _)| *name != "choosing") {
        let doc = Doc::new().with(card);
        for environment in ENVIRONMENTS.iter().chain(&RICH) {
            let drawn = Terminal::on(Look::of(environment)).draw(&doc);
            assert_golden(&environment.golden(&goldens(), name), &drawn);
        }
    }
}
