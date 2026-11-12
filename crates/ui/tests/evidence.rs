//! What a run leaves, from its stage: a finished run's receipt written beside it as
//! `yunta receipt` writes it, its page for a browser written as `yunta open` writes it
//! and asked to be shown where the API wrote it, and a broken run's evidence verified as
//! `yunta verify` says it.

mod support;

use std::path::Path;
use std::sync::Arc;

use surco::surface::{Markdown, Surface};
use yunta_core::events::{
    EventDraft, EventPayload, NodeEvent, NodeFinishedPayload, NodeOutcome, TokenUsage,
};
use yunta_core::NodeId;
use yunta_render::page::{self, Page, Verified};
use yunta_storage::Storage;
use yunta_testkit::Bench;
use yunta_testkit_ui::driving::drive;
use yunta_testkit_ui::sandbox::{now, workspace};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Performer, Request, State};

use support::{on_the_field, stopped};

/// Two nodes that finish, one after the other.
const FINISHES: &str = "name: done\nnodes:\n  - { id: first, kind: bash, run: \"true\" }\n  \
                        - { id: second, kind: bash, run: \"true\", depends_on: [first] }\n";

/// A mock fixture with no sessions: the workflow opens none.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The state root of `bench`'s repository: where its runs and log are.
fn root_of(bench: &Bench) -> &Path {
    bench
        .store_path()
        .parent()
        .expect("a bench's log stands in its root")
}

/// The screen's main region, as Markdown writes it.
fn main_region(state: &State) -> String {
    let look = Markdown.look();
    Markdown.draw(&state.screen(&look).main.doc)
}

#[tokio::test]
async fn a_finished_runs_receipt_is_the_one_receipt_writes() {
    let bench = Bench::with_run_id("01M7EV1DENCERECE1PT0000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    // The run opened, then the receipt it offers once it finished.
    let strokes = [Stroke::Enter, Stroke::Insert('e')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let written = |name: &str| {
        std::fs::read_to_string(bench.run_dir().join(name)).expect("the receipt written")
    };
    let (markdown, json) = (written("receipt.md"), written("receipt.json"));
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let receipt = project
        .run(&bench.run_id)
        .await
        .expect("the run")
        .receipt()
        .expect("the receipt `yunta receipt` writes");
    assert_eq!(written("receipt.md"), markdown);
    assert_eq!(written("receipt.json"), json);
    let look = Markdown.look();
    let printed = Markdown.draw(&page::Receipt { receipt: &receipt }.document(&look));
    assert_eq!(main_region(&state), printed);
}

#[tokio::test]
async fn a_runs_page_is_shown_where_open_writes_it() {
    let bench = Bench::with_run_id("01M7EV1DENCEPAGE0000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    let strokes = [Stroke::Enter, Stroke::Insert('o')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    let page = bench.run_dir().join("page.html");
    assert_eq!(surfaced, [Request::ShowPage(page.clone())]);
    assert!(page.is_file(), "the page is written before it is shown");
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail.last().map(String::as_str), Some("page"));
}

/// A page the surface could not show is said, with where it is, in the words `yunta open`
/// says it in.
#[tokio::test]
async fn a_page_not_shown_is_said_with_where_it_is() {
    let bench = Bench::with_run_id("01M7EV1DENCEN0TSH0WN000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    let strokes = [Stroke::Enter, Stroke::Insert('o')];
    let (mut state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    let [Request::ShowPage(page)] = surfaced.as_slice() else {
        panic!("the page is shown: {surfaced:?}");
    };
    assert!(state.update(Input::Unshown(page.clone())).is_empty());
    let said = Markdown.screen(&state.screen(&Markdown.look()));
    let opened = page.display().to_string();
    let printed = Markdown.draw(
        &page::Opened {
            page: &opened,
            opened: Some(false),
        }
        .document(&Markdown.look()),
    );
    assert!(said.contains(printed.trim()), "{said}\nholds\n{printed}");
}

/// `bench`'s run with a node that finished without ever starting appended to its log,
/// behind the engine's back: replay reads the run as broken there.
fn broken(bench: &Bench) {
    let storage = Storage::open(bench.store_path()).expect("the bench's log");
    let usage = TokenUsage {
        input: 0,
        output: 0,
        cached: None,
    };
    let finished = NodeFinishedPayload::new(NodeOutcome::Summary("ghost".to_string()), usage);
    let payload = EventPayload::Node(NodeEvent::Finished(finished));
    let ghost = Some(NodeId::from("ghost"));
    let draft = EventDraft::new(bench.run_id.clone(), ghost, payload).expect("a draft");
    storage
        .append_at(&draft, now())
        .expect("the event appended");
}

#[tokio::test]
async fn a_broken_runs_evidence_is_verified_as_verify_says_it() {
    let bench = Bench::with_run_id("01M7BROKEN0000000000000000");
    bench.run(FINISHES, NO_SESSIONS).await;
    broken(&bench);
    let performer = Arc::new(Performer::new(workspace(&bench.worktree, root_of(&bench))));
    let field = on_the_field(vec![stopped(&bench.run_id)]);
    // The run opened: a broken run offers its evidence verified, on `v`.
    let strokes = [Stroke::Enter, Stroke::Insert('v')];
    let (state, surfaced) = drive(field, Vec::new(), &performer, &strokes).await;
    assert!(surfaced.is_empty(), "{surfaced:?}");
    let project = workspace(&bench.worktree, root_of(&bench))
        .project()
        .expect("the project");
    let verification = project
        .verification(&bench.run_id)
        .await
        .expect("the run verified");
    let said = Verified {
        run_id: &bench.run_id,
        checks: &verification.checks(),
    };
    assert_eq!(
        main_region(&state),
        Markdown.draw(&said.document(&Markdown.look()))
    );
    let trail = state.screen(&Markdown.look()).trail;
    assert_eq!(trail.last().map(String::as_str), Some("verification"));
}
