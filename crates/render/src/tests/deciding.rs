//! What weighs on a decision, said beside its menu, and what a decision
//! shows besides the plan — drawn on every surface and held to their
//! goldens.

use std::path::PathBuf;

use yunta_core::shown::PlanReview;
use yunta_testkit_core::golden::{assert_golden, ENVIRONMENTS};

use super::documents::{goldens, review};
use crate::plan::Form;
use crate::surface::{Markdown, Surface, Terminal};
use crate::Look;

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
    let shown = [shown(review)];
    let withheld = [yunta_core::events::Withheld {
        option: "approve".into(),
        because: "the plan cannot be proven as it is written".to_string(),
    }];
    let before = crate::shown::before_you_decide(&withheld, &shown)
        .expect("a plan that cannot be proven weighs on the decision");
    let doc = crate::doc::Doc::new().with(before);
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

/// `review` as a gate shows it.
fn shown(review: PlanReview) -> yunta_core::shown::ShownDocument {
    yunta_core::shown::ShownDocument {
        shown: yunta_core::events::Shown {
            producer: Some("plan".into()),
            artifact: yunta_core::events::ArtifactId::Interpreted {
                kind: yunta_core::ArtifactKind::Tasks,
            },
            content_hash: yunta_core::ContentHash::sha256(b"plan"),
        },
        path: PathBuf::from("artifacts/plan/tasks.yaml"),
        content: yunta_core::shown::ShownContent::Tasks(Box::new(review)),
    }
}

#[test]
fn a_pull_request_says_what_weighs_on_its_review_before_how_to_answer_it() {
    let request = yunta_core::port::PublishRequest {
        branch: "yunta/7E5PH4".to_string(),
        base_branch: "main".to_string(),
        run_id: "01K3W48MFW7H0ZZA5PZ07E5PH4".parse().unwrap(),
        decision: yunta_core::port::GateDecision {
            node: "approve-plan".into(),
            question: "Plan registered. Approve?".to_string(),
            assignee: "lead".to_string(),
            then: Vec::new(),
            corrected_by: Some("plan".into()),
        },
        artifacts: Vec::new(),
        shown: vec![shown(review())],
    };

    let body = Markdown.draw(&crate::published::gate(&request));

    let before = body.find("before you decide").expect("what weighs on it");
    let answer = body.find("how to answer").expect("how to answer it");
    assert!(before < answer, "{body}");
    assert!(
        body[before..answer].contains("risk: A name with a newline"),
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
            "7E5PH4",
            Form::Review,
        ))
    };

    let headed = read("# Global pack installation\n\nA pack installs for the user.\n");
    assert_eq!(
        headed.lines().next(),
        Some("Global pack installation — brief.md of `brief`"),
        "{headed}"
    );
    assert!(
        !headed.contains("# Global") && headed.contains("A pack installs for the user."),
        "the heading is the title, said once: {headed}"
    );

    let plain = read("A pack installs for the user.\n");
    assert_eq!(plain.lines().next(), Some("brief.md of `brief`"), "{plain}");
}
