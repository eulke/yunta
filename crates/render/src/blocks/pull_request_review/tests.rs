//! A gate's pull request as its scene says it: its link, then how it stands by what the
//! forge answered, a review deciding only the commit it reviewed, and why where no forge
//! answered.

use surco::doc::Doc;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::surface::Terminal;
use yunta_core::port::{ForgeUnanswered, PolledGate, PullRequestRef, ReviewOutcome};
use yunta_core::CommitSha;

use super::PullRequestReview;

/// The commit the pull request stands at.
const HEAD: &str = "1111111111111111111111111111111111111111";

/// A commit before it, which a review left on it reviewed.
const EARLIER: &str = "2222222222222222222222222222222222222222";

/// The pull request the small runs' gate publishes.
fn seven() -> PullRequestRef {
    PullRequestRef {
        url: "https://github.com/acme/web/pull/7".to_string(),
        number: 7,
    }
}

/// `sha` read as a commit.
fn commit(sha: &str) -> CommitSha {
    sha.parse().expect("a commit's sha")
}

/// The cells a line is drawn in here: wide enough for every row whole.
const CELLS: usize = 120;

/// The pull request's rows as a line `CELLS` wide draws them, its forge having answered
/// `answered`.
fn rows(answered: &Result<PolledGate, ForgeUnanswered>) -> Vec<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        width: Width::exact(CELLS),
        ..Look::plain()
    };
    let pull_request = seven();
    let review = PullRequestReview {
        pull_request: &pull_request,
        answered,
    };
    let lines = Terminal::on(look).lines(&Doc::new().with_all(review.blocks()));
    lines.iter().map(|line| line.text()).collect()
}

/// How the pull request stands where its forge answered a poll with `review`.
fn stands(review: ReviewOutcome) -> String {
    let polled = Ok(PolledGate {
        head_sha: commit(HEAD),
        review,
    });
    let rows = rows(&polled);
    let [_, stands] = rows.as_slice() else {
        panic!("the link, then how it stands: {rows:#?}");
    };
    stands.clone()
}

/// How the pull request stands where no forge answered, for `why`.
fn not_answered(why: ForgeUnanswered) -> String {
    let rows = rows(&Err(why));
    rows.last().cloned().unwrap_or_default()
}

#[test]
fn a_pull_request_no_review_decides_waits_on_its_review() {
    let polled = Ok(PolledGate {
        head_sha: commit(HEAD),
        review: ReviewOutcome::Pending,
    });
    assert_eq!(
        rows(&polled),
        [
            "pull request #7: https://github.com/acme/web/pull/7",
            "stands       waiting on its review",
        ]
    );
}

#[test]
fn a_review_of_its_last_commit_says_who_decided_it() {
    let approved = ReviewOutcome::Approved {
        by: "ana".into(),
        reviewed_sha: commit(HEAD),
    };
    assert_eq!(stands(approved), "stands       approved by ana");
    let changes = ReviewOutcome::ChangesRequested {
        by: "ana".into(),
        reviewed_sha: commit(HEAD),
        comments: Vec::new(),
    };
    assert_eq!(stands(changes), "stands       changes asked by ana");
}

/// A review left on an earlier commit decides nothing on the engine's next wake: the pull
/// request waits on a review of what it stands at now.
#[test]
fn a_review_of_an_earlier_commit_leaves_it_waiting_on_a_review_of_its_last() {
    let approved = ReviewOutcome::Approved {
        by: "ana".into(),
        reviewed_sha: commit(EARLIER),
    };
    let changes = ReviewOutcome::ChangesRequested {
        by: "ana".into(),
        reviewed_sha: commit(EARLIER),
        comments: Vec::new(),
    };
    for review in [approved, changes] {
        assert_eq!(
            stands(review),
            "stands       waiting on a review of its last commit"
        );
    }
}

#[test]
fn a_merge_or_a_close_stands_whatever_was_reviewed() {
    let merged = ReviewOutcome::Merged {
        by: "ana".into(),
        merge_sha: commit(EARLIER),
    };
    assert_eq!(stands(merged), "stands       merged by ana");
    assert_eq!(
        stands(ReviewOutcome::Closed),
        "stands       closed without merging"
    );
}

#[test]
fn where_no_forge_answered_it_says_why() {
    assert_eq!(
        not_answered(ForgeUnanswered::Undeclared),
        "stands       not known: the run declares no forge"
    );
    let unset = ForgeUnanswered::TokenUnset {
        token_env: "ACME_TOKEN".to_string(),
    };
    assert_eq!(
        not_answered(unset),
        "stands       not known: `ACME_TOKEN`, the variable its forge's token is in, is not set here"
    );
    let failed = ForgeUnanswered::Failed {
        said: "forge: no gate #7".to_string(),
    };
    assert_eq!(
        not_answered(failed),
        "stands       not known: forge: no gate #7"
    );
}
