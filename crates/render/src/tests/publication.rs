//! A gate published to a forge, as its pull request says it: the title it is
//! named by, the description it carries within the forge's limits, and the
//! files it carries beside it.

use std::path::{Path, PathBuf};

use surco::glyphs::Glyphs;
use surco::golden::assert_golden;
use surco::words::Standing;
use yunta_core::events::{ChangesCountedPayload, Claim, GateFolder};
use yunta_core::port::{GateDecision, PublishRequest};
use yunta_core::shown::PlanReview;
use yunta_core::step::NextStep;
use yunta_core::{NodeId, RunId};
use yunta_testkit_core::spell;

use super::documents::{global_packs, shown_plan};
use crate::published::{self, Publication};

/// The marker a description ends on, as the adapter writes it for the run.
const TAIL: &str = "<!-- yunta run_id: 01K3W48MFW7H0ZZA5PZ07E5PH4 -->";
/// The run the pull request is for: the id the marker names.
const PULL_REQUEST_RUN: &str = "01K3W48MFW7H0ZZA5PZ07E5PH4";
/// Where the whole plan of that run's gate is, on the branch of its pull request.
const WHOLE_PLAN: &str = ".yunta/gates/01K3W48MFW7H0ZZA5PZ07E5PH4/approve-plan/tasks.md";
/// Where the run's tasks file is, on the branch of its pull request.
const TASKS_YAML: &str = ".yunta/gates/01K3W48MFW7H0ZZA5PZ07E5PH4/approve-plan/tasks.yaml";
/// Where the run's spec file is, on the branch of its pull request.
const SPEC_YAML: &str = ".yunta/gates/01K3W48MFW7H0ZZA5PZ07E5PH4/approve-plan/spec.yaml";
/// The tasks file the run holds for the plan of global packs.
const TASKS_BYTES: &[u8] = include_bytes!("fixtures/global-packs.tasks.yaml");
/// The spec file the run holds for the plan of global packs.
const SPEC_BYTES: &[u8] = include_bytes!("fixtures/global-packs.spec.yaml");
/// The most characters GitHub keeps in a description: its API refuses a longer one.
const BODY_LIMIT: usize = 65_536;
/// The most characters GitHub keeps in a title: a title past it is refused, so
/// it is cut to it.
const TITLE_LIMIT: usize = 256;
/// The bytes a walk over the limits steps by once the question is whole: small
/// enough to meet every size a description takes on the way, large enough that
/// the walk is quick.
const STEP: usize = 500;
/// A summary longer than a title can hold, so that the title is cut while the
/// description keeps the summary whole.
const LONG_SUMMARY: usize = 300;
/// What a title says before the change it names.
const PREFIX: &str = "Approve: ";

/// The directory the gate's goldens are kept in.
pub(super) fn gate_goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/gate")
}

/// GitHub as a gate's pull request names it, with `body_limit` as the most its
/// description keeps.
fn publication(spell: &dyn Fn(&NextStep) -> String, body_limit: usize) -> Publication<'_> {
    Publication {
        forge: "GitHub",
        body_limit,
        spell,
    }
}

/// The request a gate of `review` publishes: the plan as the document the gate
/// shows, the two files the run holds beside it, and the decision that asks to
/// approve the change the plan names.
fn publishing(review: &PlanReview) -> PublishRequest {
    let run_id = RunId::from_static(PULL_REQUEST_RUN);
    let node = NodeId::from("approve-plan");
    let folder = GateFolder::of(&run_id, &node);
    PublishRequest {
        branch: "yunta/7E5PH4".to_string(),
        base_branch: "main".to_string(),
        run_id,
        folder,
        decision: GateDecision {
            node,
            question: Claim::ApprovePlan {
                summary: review
                    .plan
                    .summary
                    .clone()
                    .expect("the plan names its change"),
            },
            assignee: "lead".to_string(),
            then: vec![NodeId::from("implement")],
            corrected_by: Some(NodeId::from("plan")),
        },
        artifacts: vec![
            ("tasks.yaml".to_string(), TASKS_BYTES.to_vec()),
            ("spec.yaml".to_string(), SPEC_BYTES.to_vec()),
        ],
        shown: vec![shown_plan(review.clone())],
        changed: None,
    }
}

/// The description `request` is published with, within GitHub's limit.
pub(super) fn description(request: &PublishRequest) -> String {
    described(request, BODY_LIMIT)
}

/// The description `request` is published with when the forge keeps `limit`
/// of it.
fn described(request: &PublishRequest, limit: usize) -> String {
    let spelling = spell();
    published::body(request, &publication(&*spelling, limit), TAIL)
}

/// The title `request` is published under, within GitHub's limit.
fn titled(request: &PublishRequest) -> String {
    published::title(request, TITLE_LIMIT)
}

/// The least limit under which the description holds `holds`. A description
/// leaves out more as its limit shrinks, so halving the range finds the limit
/// where what `holds` looks for first fits.
fn least_limit(request: &PublishRequest, holds: impl Fn(&str) -> bool) -> usize {
    least_where(|limit| holds(&described(request, limit)))
}

/// The least limit at which `holds` is true, given that it stays true from
/// there up: halving the range finds that limit.
fn least_where(holds: impl Fn(usize) -> bool) -> usize {
    let (mut low, mut high) = (0, BODY_LIMIT);
    while high - low > 1 {
        let middle = (low + high) / 2;
        if holds(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

/// The least limit under which the description fits and holds the close row of
/// its answers: the decision with its question cut to an ellipsis, the least
/// body a description keeps.
fn least_body(request: &PublishRequest) -> usize {
    least_where(|limit| {
        let page = described(request, limit);
        page.len() <= limit && page.contains("| **close** |")
    })
}

/// Asserts that `page`, a description, is within the `limit` bytes the forge keeps.
fn assert_within(page: &str, limit: usize) {
    assert!(
        page.len() <= limit,
        "{} bytes at a limit of {limit}",
        page.len()
    );
}

/// The trimmed lines of each test the spec wrote, where a line says more than
/// a closing brace.
fn spec_lines(review: &PlanReview) -> Vec<&str> {
    review
        .spec
        .iter()
        .flat_map(|spec| &spec.specs)
        .flat_map(|spec| &spec.files)
        .flat_map(|file| file.content.lines())
        .map(str::trim)
        .filter(|line| line.len() > 2)
        .collect()
}

/// What the description of the plan of global packs says beyond its golden: the
/// implication said once, each card and the proof folded, the risk said once,
/// no command the page leaves out on purpose (the spec's test command, the
/// prompt's command, and the cancel that stops nothing on a parked run), no
/// line of a test the spec wrote, the decision once, and the limit held.
fn assert_plain_facts(page: &str, review: &PlanReview) {
    let implied = "- implies: resolve_workflow is named in 5 files no task changes:";
    assert_eq!(page.matches(implied).count(), 1, "{page}");
    let modified = format!("_modified {} 1 line_", Glyphs::Unicode.sep());
    assert!(page.contains(&modified), "{page}");
    assert_eq!(page.matches("<summary><b>").count(), 4, "{page}");
    assert_eq!(page.matches("<details>").count(), 5, "{page}");
    assert_eq!(page.matches("risk: Global lock state").count(), 1, "{page}");
    for command in [
        "--node specify",
        "resolve-gate",
        "yunta cancel",
        "the gate fails, and the run goes no further",
    ] {
        assert!(!page.contains(command), "`{command}` is said:\n{page}");
    }
    for line in spec_lines(review) {
        assert!(
            !page.contains(line),
            "`{line}` is a line of a test:\n{page}"
        );
    }
    assert_eq!(
        page.matches("on the machine that holds the run").count(),
        1,
        "{page}"
    );
    assert!(page.len() <= BODY_LIMIT, "{} bytes", page.len());
}

/// A gate's pull request reads as its page: the description its golden holds,
/// and what the page says beyond that golden.
#[test]
fn a_gate_s_pull_request_reads_as_its_page() {
    let review = global_packs();
    let page = description(&publishing(&review));
    let drawn = page
        .strip_suffix(TAIL)
        .and_then(|head| head.strip_suffix("\n\n"))
        .expect("the marker ends the description");
    assert_golden(
        &gate_goldens().join("pull-request.md"),
        &format!("{drawn}\n"),
    );
    assert_plain_facts(&page, &review);
}

/// A pull request says what the run changed, as git last counted it, above the
/// documents it is about; a request nothing counted says nothing of the change.
#[test]
fn a_pull_request_says_what_the_run_changed_above_its_documents() {
    let mut request = publishing(&global_packs());
    request.changed = Some(ChangesCountedPayload {
        tree: "deadbeef".parse().unwrap(),
        files: 9,
        added: 412,
        removed: 87,
    });
    let [plus, minus] = Glyphs::Unicode.changed();
    let row = format!("- changed: 9 files, {plus}412 {minus}87");
    let body = description(&request);
    let documents = body.find("## plan:").expect("the plan is drawn");
    assert!(
        body.find(row.as_str()).is_some_and(|at| at < documents),
        "{body}"
    );
    assert!(
        !description(&publishing(&global_packs())).contains("- changed:"),
        "a request nothing counted says nothing of the change"
    );
}

/// A pull request carries the whole plan drawn as a file, then each file the
/// run holds exactly as the run holds it, every one at its path in the gate's
/// folder.
#[test]
fn the_pull_request_carries_the_run_s_files_unchanged() {
    let request = publishing(&global_packs());
    let spelling = spell();
    let files = published::files(&request, &*spelling);
    let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, [WHOLE_PLAN, TASKS_YAML, SPEC_YAML]);
    assert_eq!(files[1].1, TASKS_BYTES);
    assert_eq!(files[2].1, SPEC_BYTES);
}

/// One character over the forge's limit, a description leaves the lines of its
/// code blocks out first: its folds stay, and its top says what it left out.
#[test]
fn a_description_too_long_leaves_its_code_lines_out_first() {
    let request = publishing(&global_packs());
    let limit = description(&request).len() - 1;
    let page = described(&request, limit);
    assert_within(&page, limit);
    assert!(page.lines().next().is_some_and(|l| l.contains("7E5PH4")));
    assert!(!page.contains("```rust"), "{page}");
    assert_eq!(page.matches("<summary><b>").count(), 4, "{page}");
    let note = format!(
        "This description leaves out the lines of each code block, since GitHub keeps \
         a pull request's description to {limit} characters; the files it carries hold \
         every document whole."
    );
    assert!(page.contains(&note), "{page}");
}

/// Where leaving out the lines of the code is not enough to fit, each card is
/// drawn as its title alone, and the note says what each folded section holds.
#[test]
fn a_description_too_long_for_its_code_leaves_its_folds_out() {
    let request = publishing(&global_packs());
    let folds_out = least_limit(&request, |page| page.contains("## steps"));
    let code_out = least_limit(&request, |page| page.contains("<details>"));
    let limit = (folds_out + code_out) / 2;
    let page = described(&request, limit);
    assert_within(&page, limit);
    assert!(page.lines().next().is_some_and(|l| l.contains("7E5PH4")));
    assert!(!page.contains("<details>"), "{page}");
    assert!(
        page.contains("**global-pack-catalog**: Discover global packs after project packs"),
        "{page}"
    );
    assert!(page.contains("what each folded section holds"), "{page}");
}

/// Just over the page that has no document in it, the description leaves its
/// steps out, keeps the table of answers, and says which documents it left out.
#[test]
fn a_description_too_long_for_its_folds_leaves_its_documents_out() {
    let request = publishing(&global_packs());
    let lean = least_limit(&request, |page| page.contains("before you decide"));
    let limit = lean + 1;
    let page = described(&request, limit);
    assert_within(&page, limit);
    assert!(page.lines().next().is_some_and(|l| l.contains("7E5PH4")));
    assert!(!page.contains("## steps"), "{page}");
    assert!(
        page.contains("| answer | on GitHub | what happens |"),
        "{page}"
    );
    let note = format!(
        "This description leaves out the documents the gate shows, since GitHub keeps \
         a pull request's description to {limit} characters; the files it carries hold \
         every document whole."
    );
    assert!(page.contains(&note), "{page}");
}

/// The description fits every limit from the least body to the whole question, and each STEP above.
#[test]
fn a_body_never_passes_its_limit() {
    let request = publishing(&global_packs());
    let least = least_body(&request);
    let alone = least_limit(&request, |page| page.contains("across projects?"));
    let full = description(&request).len();
    // Each limit where the question is cut is checked, then every STEP up to the
    // whole page.
    let limits = (least..=alone)
        .chain((alone..=full).step_by(STEP))
        .chain([full]);
    for limit in limits {
        assert_within(&described(&request, limit), limit);
    }
}

/// The least body is the decision with its question cut to an ellipsis.
#[test]
fn the_least_body_is_the_decision_with_its_question_cut_to_an_ellipsis() {
    let request = publishing(&global_packs());
    let least = least_body(&request);
    let page = described(&request, least);
    let mark = Glyphs::Unicode.mark(Standing::NeedsYou.mark());
    let cut = format!("## {mark} {}", Glyphs::Unicode.ellipsis());
    assert!(page.contains(&cut), "{page}");
    assert_answers_and_commands(&page);
    assert!(
        described(&request, least - 1).len() > least - 1,
        "a body one byte under the least limit fits it"
    );
}

/// Words of 70,000 characters, more than a description keeps beside its decision.
fn heavy_words() -> String {
    "burden ".repeat(10_000)
}

/// Asserts that `body` holds its answers: the rows of approve, request changes
/// and close, and both commands of the machine that holds the run.
fn assert_answers_and_commands(body: &str) {
    for answer in [
        "| **approve** |",
        "| **request changes** |",
        "| **close** |",
    ] {
        assert!(body.contains(answer), "`{answer}` is not drawn:\n{body}");
    }
    for command in ["yunta resume 7E5PH4", "yunta close 7E5PH4"] {
        assert!(body.contains(command), "`{command}` is not drawn:\n{body}");
    }
}

/// A plan whose only risk is too long for the body still holds every answer and both commands.
#[test]
fn a_decision_too_heavy_for_the_body_still_holds_its_answers() {
    let mut review = global_packs();
    review.plan.risks = vec![heavy_words()];
    let body = description(&publishing(&review));
    assert_within(&body, BODY_LIMIT);
    assert_answers_and_commands(&body);
    assert!(body.contains("what weighs on the decision"), "{body}");
    assert!(
        !body.contains("burden"),
        "a part of the risk is said:\n{body}"
    );
}

/// A question too long for the body is cut with an ellipsis, and its answers stay.
#[test]
fn a_question_too_long_for_the_body_is_cut_and_its_answers_kept() {
    let mut review = global_packs();
    review.plan.summary = Some(heavy_words());
    let body = description(&publishing(&review));
    assert_within(&body, BODY_LIMIT);
    let mark = Glyphs::Unicode.mark(Standing::NeedsYou.mark());
    let prefix = format!("## {mark} Approve the plan: burden");
    let heading = body.lines().find(|line| line.starts_with(&prefix));
    assert!(
        heading.is_some_and(|line| line.ends_with(Glyphs::Unicode.ellipsis())),
        "the decision heading is not cut with an ellipsis:\n{body}"
    );
    assert_answers_and_commands(&body);
}

/// A pull request is titled by the change its plan names, and its description
/// asks whatever question it was given.
#[test]
fn a_pull_request_is_titled_by_its_change() {
    let mut request = publishing(&global_packs());
    let change = "Install packs in the user's Yunta state directory for use across projects";
    assert_eq!(titled(&request), format!("{PREFIX}{change}"));
    request.decision.question = Claim::Written("Is this plan the one to build?".to_string());
    assert_eq!(titled(&request), format!("{PREFIX}{change}"));
    let mark = Glyphs::Unicode.mark(Standing::NeedsYou.mark());
    assert!(
        description(&request).contains(&format!("## {mark} Is this plan the one to build?")),
        "the question heads the decision"
    );
}

/// A pull request with no plan to show is titled by the words of its question.
#[test]
fn a_pull_request_with_no_plan_is_titled_by_its_question() {
    let mut request = publishing(&global_packs());
    request.shown.clear();
    request.decision.question = Claim::ApprovePlan {
        summary: "Make it".to_string(),
    };
    assert_eq!(titled(&request), "Approve the plan: Make it?");
    request.decision.question = Claim::NeedsDecision {
        node: NodeId::from("review"),
    };
    assert_eq!(titled(&request), "gate `review` needs a decision");
    request.decision.question = Claim::Written("Is the spec ready to build on?".to_string());
    assert_eq!(titled(&request), "Is the spec ready to build on?");
}

/// A title past the forge's limit is cut to it and ends on an ellipsis, while
/// the description states the whole summary.
#[test]
fn a_long_title_is_cut_and_the_body_says_it_whole() {
    let summary = "a".repeat(LONG_SUMMARY);
    let mut review = global_packs();
    review.plan.summary = Some(summary.clone());
    let request = publishing(&review);
    // Titles count characters, not cells: the ellipsis is one, and the kept ones fill the rest.
    let kept = TITLE_LIMIT - PREFIX.len() - 1;
    let ellipsis = Glyphs::Unicode.ellipsis();
    let cut = format!("{PREFIX}{}{ellipsis}", "a".repeat(kept));
    assert_eq!(titled(&request), cut);
    assert!(
        description(&request).contains(&summary),
        "the summary is said whole"
    );
}

/// A title counts characters, as GitHub does, not the cells a terminal draws.
#[test]
fn a_title_counts_characters_not_cells() {
    let mut review = global_packs();
    review.plan.summary = Some("界".repeat(LONG_SUMMARY));
    // A wide character takes two cells, so a cut in cells would keep about half as many.
    let kept = TITLE_LIMIT - PREFIX.len() - 1;
    let ellipsis = Glyphs::Unicode.ellipsis();
    let cut = format!("{PREFIX}{}{ellipsis}", "界".repeat(kept));
    let title = titled(&publishing(&review));
    assert_eq!(title.chars().count(), TITLE_LIMIT);
    assert_eq!(title, cut);
}
