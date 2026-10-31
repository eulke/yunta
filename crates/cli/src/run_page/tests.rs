use std::path::{Path, PathBuf};

use surco::doc::{Block, Depth};
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, EventPayload, NodeEvent, NodeFinishedPayload,
    NodeOutcome, NodeStartedPayload, SessionEvent, StoredEvent, TokenUsage,
};
use yunta_core::{NodeId, RunId, Workflow};
use yunta_engine::{derive, run_frame_of, RunFrame, RunState};
use yunta_testkit_core::{fixed_now, Log};

use super::fixtures::{cases, outline, Case, RUN};
use super::{RunPage, Scope};

/// Every depth a page is read at, by the name its golden carries.
const DEPTHS: [(Depth, &str); 3] = [
    (Depth::Glance, "glance"),
    (Depth::Full, "full"),
    (Depth::Dig, "dig"),
];

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/status")
}

/// The page as a terminal draws it for `look`.
fn draw(page: &RunPage<'_>, look: &Look) -> String {
    crate::render::draw(page.document(look), look)
}

#[test]
fn the_status_page_matches_its_goldens_at_every_depth() {
    for case in &cases() {
        for (depth, named) in DEPTHS {
            for environment in &ENVIRONMENTS {
                let look = Look::of(environment);
                assert_golden(
                    &environment.golden(&goldens(), &format!("{}.{named}", case.name)),
                    &draw(&case.page(Scope::Status, depth), &look),
                );
            }
        }
    }
}

#[test]
fn a_page_opens_on_its_headline() {
    for case in &cases() {
        for scope in [Scope::Status, Scope::Closing] {
            let drawn = draw(&case.page(scope, Depth::Glance), &Look::plain());
            let first = drawn.lines().next().unwrap_or_default();
            assert!(
                first.contains("fragua (quick)") && first.trim_end().ends_with(RUN.handle()),
                "{}: {drawn}",
                case.name
            );
        }
    }
}

#[test]
fn a_decision_that_waits_is_the_last_thing_on_the_page() {
    for case in cases().iter().filter(|case| case.decision.is_some()) {
        for (depth, _) in DEPTHS {
            let page = case.page(Scope::Status, depth);
            let doc = page.document(&Look::plain());
            let Some(Block::Section(last)) = doc.blocks().last() else {
                panic!("{}: the page ends on something else", case.name);
            };
            assert!(
                last.title.text().starts_with("decision on node"),
                "{}: {}",
                case.name,
                last.title.text()
            );
        }
    }
}

#[test]
fn a_glance_at_sixty_columns_fits_twenty_rows_beside_its_decision() {
    let narrow = ENVIRONMENTS
        .iter()
        .find(|environment| environment.width == 60)
        .expect("an environment sixty columns wide");
    for case in &cases() {
        let drawn = draw(&case.page(Scope::Status, Depth::Glance), &Look::of(narrow));
        // A decision is as long as its menu: what the glance owes is the
        // run above it.
        let above = drawn
            .lines()
            .take_while(|line| !line.contains("decision on node"))
            .count();
        assert!(above <= 20, "{}:\n{drawn}", case.name);
    }
}

/// What a page says at `depth`, laid out wide enough that no row wraps.
fn said(case: &Case, depth: Depth) -> String {
    draw(&case.page(Scope::Status, depth), &Look::plain())
}

#[test]
fn what_a_glance_leaves_out_is_one_depth_away() {
    let cases = cases();
    let parked = cases
        .iter()
        .find(|case| case.name == "needs-you")
        .expect("a parked case");
    let glance = said(parked, Depth::Glance);
    let full = said(parked, Depth::Full);
    let dig = said(parked, Depth::Dig);
    for (fact, at) in [
        ("tokens", &full),
        ("branch", &full),
        ("slowest", &full),
        ("global-pack-docs", &full),
        ("time", &full),
        ("kept in", &dig),
        ("objects/", &dig),
    ] {
        assert!(!glance.contains(fact), "a glance says `{fact}`: {glance}");
        assert!(at.contains(fact), "no depth says `{fact}`:\n{full}\n{dig}");
    }
    for fact in ["progress", "nodes", "tasks", "global-pack-storage"] {
        assert!(
            glance.contains(fact),
            "a glance leaves out `{fact}`: {glance}"
        );
    }
}

/// The workflow the spending run follows: two nodes, neither of them parked.
fn workflow() -> Workflow {
    yunta_core::yaml::parse(
        "name: running\nnodes:\n  - { id: plan, kind: bash, run: \"true\" }\n  - { id: build, kind: bash, run: \"true\" }\n",
    )
    .expect("a workflow with two nodes")
}

/// The log of a run whose `plan` node settled 60 input tokens and whose
/// `build` node has 40 in flight, with no output.
fn spend_log() -> Vec<StoredEvent> {
    let usage = AgentMessagePayload {
        message_type: AgentMessageType::Usage,
        tool_name: None,
        target: None,
        input_tokens: Some(40),
        output_tokens: Some(0),
        cached_input_tokens: None,
        note: None,
        text: None,
    };
    let settled = NodeFinishedPayload::new(
        NodeOutcome::Summary("exit 0".to_string()),
        TokenUsage {
            input: 60,
            output: 0,
            cached: None,
        },
    );
    Log::for_run(RUN.as_str())
        .node(
            "plan",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node("plan", EventPayload::Node(NodeEvent::Finished(settled)))
        .node(
            "build",
            EventPayload::Node(NodeEvent::Started(NodeStartedPayload::attempt(1))),
        )
        .node("build", EventPayload::Session(SessionEvent::Message(usage)))
        .build()
}

/// The page of the run `spend_log` writes. Both scopes are drawn from the
/// same frame.
fn running_page(scope: Scope) -> RunPage<'static> {
    let state: &'static RunState = Box::leak(Box::new(derive(&spend_log())));
    let frame: &'static RunFrame = Box::leak(Box::new(run_frame_of(
        &RUN,
        &workflow(),
        state,
        None,
        &yunta_engine::Engine::this(fixed_now()),
        fixed_now(),
    )));
    let run_id: &'static RunId = Box::leak(Box::new(RUN));
    RunPage {
        scope,
        depth: Depth::Glance,
        run_id,
        engine: yunta_engine::Engine::this(fixed_now()),
        frame,
        state,
        paths: outline(),
        decision: None,
        plans: Vec::new(),
    }
}

#[test]
fn the_closing_and_the_status_page_show_one_tokens_figure() {
    assert_eq!(
        running_page(Scope::Status).tokens().as_deref(),
        Some("100 in / 0 out")
    );
    assert_eq!(
        running_page(Scope::Closing).tokens(),
        running_page(Scope::Status).tokens()
    );
}

/// A plan node's row says the plan it accepted, not what its agent said last.
#[test]
fn a_plan_node_says_the_plan_it_had_accepted() {
    let mut accepted = running_page(Scope::Status);
    accepted.depth = Depth::Full;
    accepted.plans = vec![(
        NodeId::from_static("plan"),
        "Install packs from a global registry".to_string(),
    )];
    let drawn = draw(&accepted, &Look::plain());
    assert!(
        drawn.contains("Install packs from a global registry"),
        "{drawn}"
    );
    assert!(!drawn.contains("exit 0"), "{drawn}");

    let mut said = running_page(Scope::Status);
    said.depth = Depth::Full;
    assert!(draw(&said, &Look::plain()).contains("exit 0"));
}
