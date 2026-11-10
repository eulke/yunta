//! The line a promotion says: the mode alone when its log names neither a
//! cause nor a sentence, and the failure that exhausted its reroutes when
//! it names one.

use yunta_core::events::{run, Evidence, Failure, PromotionCause};

use super::super::say;

#[test]
fn a_promotion_with_no_cause_recorded_reads_as_the_mode_alone() {
    let promoted =
        yunta_replay::frame::Happening::Run(run::happening::Happening::PromotionSignaled {
            to: "ship".into(),
            cause: None,
            legacy_reason: None,
            evidence: Evidence::none(),
        });
    let said = say(
        &yunta_replay::frame::Moment::sample(None, promoted),
        surco::glyphs::Glyphs::Unicode,
    );
    assert_eq!(said.text, "run promoted to `ship`");
}

/// The failure a promotion carries is read beside the node whose
/// reroutes ran out, after the node and the mode it moves to.
#[test]
fn a_promotion_says_the_failure_that_exhausted_its_re_routes() {
    let cause = PromotionCause {
        node: "lint".into(),
        goto: "fix-lint".into(),
        failure: Some(Failure::Message {
            outcome: "exit 1".into(),
        }),
    };
    let promoted =
        yunta_replay::frame::Happening::Run(run::happening::Happening::PromotionSignaled {
            to: "ship".into(),
            cause: Some(cause),
            legacy_reason: None,
            evidence: Evidence::none(),
        });
    let said = say(
        &yunta_replay::frame::Moment::sample(None, promoted),
        surco::glyphs::Glyphs::Unicode,
    );
    assert_eq!(
        said.text,
        "run promoted to `ship`: node `lint` exhausted its reroutes to `fix-lint` (exit 1)"
    );
}
