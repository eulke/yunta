//! A fixture's declared cause is read the way its CLI reads the message:
//! the same words are a limit under Codex, and under Claude Code, which
//! reads its limit from the stream's fields, they are a final refusal.

use yunta_adapters::MockFixture;

/// The message Codex prints when the account's limit stopped the session.
const LIMIT_MESSAGE: &str = "You've hit your usage limit. Try again later.";

/// A fixture for `dialect` whose one session fails with [`LIMIT_MESSAGE`]
/// and declares the failure a final refusal.
fn final_refusal_of_the_limit(dialect: &str) -> String {
    format!(
        "dialect: {dialect}\nsessions:\n  - outcome: {{ type: failed, message: \"{LIMIT_MESSAGE}\", cause: final }}\n"
    )
}

#[test]
fn a_codex_limit_declared_as_a_final_refusal_is_refused() {
    let refused = MockFixture::parse_without_a_run(&final_refusal_of_the_limit("codex"))
        .expect_err("Codex reads this message as a limit, not as a final refusal");
    let said = refused.to_string();
    assert!(
        said.contains("final") && said.contains("limit"),
        "the refusal names both causes: {said}"
    );
}

#[test]
fn the_same_message_declared_as_a_final_refusal_is_accepted_under_claude_code() {
    MockFixture::parse_without_a_run(&final_refusal_of_the_limit("claude-code"))
        .expect("Claude Code reads this text as a final refusal");
}
