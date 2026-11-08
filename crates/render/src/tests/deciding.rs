//! What weighs on a decision, said beside its menu, and what a decision
//! shows besides the plan — drawn on every surface and held to their
//! goldens.

use std::path::PathBuf;

use surco::doc::Block;
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::events::{AuthorGate, Claim, Escalation, GateOption, GateWaitingPayload, Menu};
use yunta_core::reserved::ReservedOption;
use yunta_core::{NonEmpty, TasksFile};

use super::console_page::greet_escalation;
use super::documents::{global_packs, goldens, read, review, run, shown_plan};
use super::publication::description;
use crate::gate;
use crate::plan::Form;

#[test]
fn what_weighs_on_a_decision_matches_its_goldens() {
    let mut review = review();
    let mut handed = yunta_core::events::artifacts::HandedOver::default();
    handed.submissions = 16;
    handed.refusals = 15;
    handed.refused = vec![
        (
            yunta_core::diagnostic::DiagnosticCode::Rule(
                yunta_core::diagnostic::RuleCode::CriterionAlreadyPasses,
            ),
            12,
        ),
        (
            yunta_core::diagnostic::DiagnosticCode::Rule(
                yunta_core::diagnostic::RuleCode::SharedCriterion,
            ),
            3,
        ),
    ];
    review.handed_over = Some(handed);
    let shown = [shown_plan(review)];
    let withheld = [yunta_core::events::Withheld {
        option: "approve".into(),
        because: yunta_core::events::WhyWithheld::Logged(
            "the plan cannot be proven as it is written".into(),
        ),
    }];
    let before = crate::shown::before_you_decide(&withheld, &shown)
        .expect("a plan that cannot be proven weighs on the decision");
    let doc = surco::doc::Doc::new().with(before);
    for environment in &ENVIRONMENTS {
        assert_golden(
            &environment.golden(&goldens(), "before-you-decide"),
            &Terminal::on(Look::of(environment)).draw(&doc),
        );
    }
    assert_golden(
        &goldens().join("before-you-decide.md"),
        &Markdown.draw(&doc),
    );
}

/// What weighs on a review is said before the answers it weighs on, and each
/// risk of the plan is said there whole.
#[test]
fn a_pull_request_says_what_weighs_on_its_review_before_its_answers() {
    let request = yunta_core::port::PublishRequest {
        branch: "yunta/7E5PH4".to_string(),
        base_branch: "main".to_string(),
        run_id: "01K3W48MFW7H0ZZA5PZ07E5PH4".parse().unwrap(),
        folder: yunta_core::events::GateFolder::Root,
        decision: yunta_core::port::GateDecision {
            node: "approve-plan".into(),
            question: yunta_core::events::Claim::Written("Plan registered. Approve?".to_string()),
            assignee: "lead".to_string(),
            then: Vec::new(),
            corrected_by: Some("plan".into()),
        },
        artifacts: Vec::new(),
        shown: vec![shown_plan(review())],
        changed: None,
    };

    let body = description(&request);

    let before = body.find("before you decide").expect("what weighs on it");
    let answers = body.find("| answer |").expect("the answers a review gives");
    assert!(before < answers, "{body}");
    assert!(
        body[before..answers].contains("risk: A name with a newline"),
        "{body}"
    );
}

/// `text` as the brief a gate shows.
fn brief(text: &str) -> yunta_core::shown::ShownDocument {
    yunta_core::shown::ShownDocument {
        shown: yunta_core::events::Shown {
            producer: Some("brief".into()),
            artifact: yunta_core::events::ArtifactId::Opaque {
                name: "brief.md".to_string(),
            },
            content_hash: yunta_core::ContentHash::sha256(text.as_bytes()),
        },
        path: PathBuf::from("artifacts/brief/brief.md"),
        content: yunta_core::shown::ShownContent::Text(text.to_string()),
    }
}

#[test]
fn a_markdown_document_shown_is_titled_by_its_first_heading() {
    let read = |text: &str| {
        Terminal::on(Look::plain()).draw(&crate::shown::document(
            &brief(text),
            &run(),
            Form::Review,
        ))
    };

    let headed = read("# Global pack installation\n\nA pack installs for the user.\n");
    assert_eq!(
        headed.lines().next(),
        Some("Global pack installation: brief.md of `brief`"),
        "{headed}"
    );
    assert!(
        !headed.contains("# Global") && headed.contains("A pack installs for the user."),
        "the heading is the title, said once: {headed}"
    );

    let plain = read("A pack installs for the user.\n");
    assert_eq!(plain.lines().next(), Some("brief.md of `brief`"), "{plain}");
}

/// With no plan on the page, nothing else says why the gate withholds approve,
/// so the record names the option and the reason it is withheld.
#[test]
fn a_withheld_reason_is_recorded_when_no_plan_is_shown() {
    let escalation = greet_escalation(&review());
    let record = crate::escalation::record(&escalation, &[]);
    assert!(
        record
            .iter()
            .any(|line| line.starts_with("withheld: `approve`, because")),
        "{}",
        record.join("\n")
    );
}

/// A plan on the page that has the flaws says why approve is withheld, once,
/// under the section that names them; the record leaves the reason out.
#[test]
fn a_plan_with_flaws_says_why_an_option_is_withheld_once_under_the_section() {
    let shown = [shown_plan(review())];
    let escalation = greet_escalation(&review());
    let record = crate::escalation::record(&escalation, &shown);
    assert!(
        !record.iter().any(|line| line.contains("withheld")),
        "{}",
        record.join("\n")
    );
    let section =
        crate::shown::before_you_decide(escalation.withheld(), &shown).expect("what weighs on it");
    let drawn = Terminal::on(Look::plain()).draw(&surco::doc::Doc::new().with(section));
    assert_eq!(
        drawn.matches("is not offered, because").count(),
        1,
        "{drawn}"
    );
}

/// A browser has the room a terminal does not and runs no command: its plan
/// holds each test its spec wrote, and leaves what weighs on the decision to the
/// decision.
#[test]
fn a_browser_reads_the_tests_whole_and_leaves_what_weighs_to_the_decision() {
    let drawn = read(&review(), Form::Browser);
    let tests = drawn
        .find("the spec's tests")
        .unwrap_or_else(|| panic!("no test is drawn whole:\n{drawn}"));
    assert!(drawn[tests..].contains("tests/greet.rs"), "{drawn}");
    assert!(!drawn.contains("--node"), "{drawn}");
    assert!(
        !drawn.contains("`greet` plans to change `tests/greet.rs`, a test the spec wrote"),
        "said beside the decision:\n{drawn}"
    );
    assert!(
        !read(&global_packs(), Form::Browser).contains("Global lock state"),
        "a risk is said beside the decision"
    );
}

/// What stands above a decision's card: what weighs on it, and the documents of a decision
/// with no plan. A plan is read at its desk.
#[test]
fn the_page_above_a_card_does_not_carry_a_plan() {
    let greet = review();
    let escalation = greet_escalation(&greet);
    let shown = vec![shown_plan(greet)];
    let run = run();
    let page = gate::above(Some((&run, shown.as_slice())), &escalation)
        .expect("what weighs on the decision");
    assert!(
        matches!(page.blocks(), [Block::Section(section)] if section.title.text() == "before you decide"),
    );

    let quiet: TasksFile = yunta_core::yaml::parse(
        "summary: Quiet\ntasks:\n  - { id: a, title: A, scope: [a], criteria: [{ cmd: 'true' }] }\n",
    )
    .unwrap();
    let mut nothing_weighs = review();
    nothing_weighs.plan = quiet;
    nothing_weighs.tasks = Vec::new();
    let shown = vec![shown_plan(nothing_weighs)];
    assert!(
        gate::above(Some((&run, shown.as_slice())), &withholding_nothing()).is_none(),
        "with nothing weighing, the card opens on its question"
    );

    let page = gate::whole_page(Some((&run, shown.as_slice())), &withholding_nothing())
        .expect("what a terminal with no room for a desk prints");
    assert!(matches!(page.blocks().first(), Some(Block::Headline(_))));
}

/// A decision that keeps nothing from a person.
fn withholding_nothing() -> GateWaitingPayload {
    Escalation::new(
        Claim::ApprovePlan {
            summary: "Quiet".to_string(),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".into(),
            node: None,
        }),
        NonEmpty::from((GateOption::reserved(ReservedOption::Abort), Vec::new())),
    )
    .into_payload()
}
