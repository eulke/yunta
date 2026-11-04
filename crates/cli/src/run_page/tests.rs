use std::path::{Path, PathBuf};
use std::time::Duration;

use surco::doc::{Block, Depth};
use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::{Look, Width};
use surco::words::Standing;
use yunta_core::events::{
    AgentMessagePayload, AgentMessageType, ChangesCountedPayload, EventPayload, Failure, NodeEvent,
    NodeFinishedPayload, NodeOutcome, NodeStartedPayload, RunEvent, SessionEvent, StoredEvent,
    TokenUsage,
};
use yunta_core::{NodeId, OnFailure, RunId, Workflow};
use yunta_engine::{
    derive, run_frame_of, Declares, GateAsks, NodeFrame, NodeStanding, NodeState, RunFrame,
    RunState,
};
use yunta_testkit::{node_frame, run_frame};
use yunta_testkit_core::{fixed_now, Log};

use super::fixtures::{blocked_log, cases, outline, Case, RUN};
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

/// Every page as a browser reads it. A golden holds what a page says, not
/// what dresses it.
#[test]
fn the_status_page_reads_in_a_browser() {
    use surco::surface::Html;
    let look = Html.look();
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/open");
    for case in &cases() {
        assert_golden(
            &goldens.join(format!("{}.html", case.name)),
            &Html.body(&case.page(Scope::Browser, Depth::Full).document(&look)),
        );
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
        action: None,
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

/// The one commit a person may want from a blocked task is named where the task is, with
/// the command that shows it.
#[test]
fn a_blocked_task_says_where_its_work_is() {
    let mut page = page_of(blocked_log(), Scope::Status);
    page.depth = Depth::Full;
    let drawn = draw(&page, &Look::plain());
    assert!(drawn.contains("its work is at git show deadbee"), "{drawn}");
}

/// The page of the run `events` write, in `scope`.
fn page_of(events: Vec<StoredEvent>, scope: Scope) -> RunPage<'static> {
    let state: &'static RunState = Box::leak(Box::new(derive(&events)));
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

/// The page of the run `spend_log` writes. Both scopes are drawn from the
/// same frame.
fn running_page(scope: Scope) -> RunPage<'static> {
    page_of(spend_log(), scope)
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

/// The log of a run whose change git counted on its one tree as `files`
/// files, `added` lines in and `removed` out.
fn counted(files: u32, added: u64, removed: u64) -> Vec<StoredEvent> {
    Log::for_run(RUN.as_str())
        .event(EventPayload::Run(RunEvent::ChangesCounted(
            ChangesCountedPayload {
                tree: "deadbeef".into(),
                files,
                added,
                removed,
                largest: Vec::new(),
            },
        )))
        .build()
}

/// A closing says what the run changed, as git last counted it, in a row
/// of its own; a run whose change was never counted, or touched no file,
/// says nothing of it.
#[test]
fn a_closing_says_what_the_run_changed() {
    let row = |page: &RunPage<'_>| {
        draw(page, &Look::plain())
            .lines()
            .find(|line| line.trim_start().starts_with("changed"))
            .map(|line| line.trim().to_string())
    };
    let changed = row(&page_of(counted(9, 412, 87), Scope::Closing)).unwrap_or_default();
    assert!(changed.ends_with("9 files, +412 -87"), "{changed}");
    assert_eq!(row(&running_page(Scope::Closing)), None);
    assert_eq!(row(&page_of(counted(0, 0, 0), Scope::Closing)), None);
}

/// A node the run finished with the outcome `exit 0`, and no tokens spent.
fn finished() -> NodeStanding {
    NodeStanding::Reached(NodeState::Finished {
        outcome: NodeOutcome::Summary("exit 0".to_string()),
        tokens: TokenUsage::default(),
    })
}

/// A gate that writes no question of its own: a person decides what its
/// node shows.
fn gate() -> Declares {
    Declares {
        gate: Some(GateAsks {
            question: None,
            on_forge: false,
        }),
        ..Declares::default()
    }
}

/// Five nodes of the trace, each in the state its row says of it: one
/// finished on its third attempt after two reroutes, one that asked a
/// person over two rounds, a gate a person decided once and now waits on,
/// and two nodes the run has not started.
fn trace_nodes() -> Vec<NodeFrame> {
    let paused = || NodeStanding::Paused { attempt: 1 };
    vec![
        // Finished, so its row says its attempts as a count: a node running
        // or paused on an attempt says that attempt in its state instead.
        NodeFrame {
            reroutes: 2,
            attempt: Some(3),
            ..node_frame(&NodeId::from_static("build"), finished())
        },
        NodeFrame {
            people: Duration::from_secs(724),
            question_rounds: 2,
            ..node_frame(&NodeId::from_static("ask"), paused())
        },
        NodeFrame {
            standing: Standing::NeedsYou,
            people: Duration::from_secs(60),
            decisions: 1,
            declares: gate(),
            ..node_frame(&NodeId::from_static("approve"), paused())
        },
        NodeFrame {
            declares: Declares {
                on_failure: Some(OnFailure {
                    goto: NodeId::from_static("fix-lint"),
                    max_reroutes: 2,
                }),
                ..Declares::default()
            },
            ..node_frame(&NodeId::from_static("lint"), NodeStanding::ToGo)
        },
        NodeFrame {
            declares: gate(),
            ..node_frame(&NodeId::from_static("check"), NodeStanding::ToGo)
        },
    ]
}

/// Three nodes a correction or a retry has touched: one whose failure is
/// being corrected, and two on their second attempt, one paused and one
/// running.
fn corrected_nodes() -> Vec<NodeFrame> {
    let failed = NodeStanding::Reached(NodeState::Failed {
        failure: Failure::Message {
            outcome: "exit 1".into(),
        },
        tokens: TokenUsage::default(),
    });
    vec![
        NodeFrame {
            standing: Standing::Rerouted,
            reroutes: 1,
            declares: Declares {
                on_failure: Some(OnFailure {
                    goto: NodeId::from_static("fix-tidy"),
                    max_reroutes: 1,
                }),
                ..Declares::default()
            },
            ..node_frame(&NodeId::from_static("tidy"), failed)
        },
        NodeFrame {
            attempt: Some(2),
            ..node_frame(
                &NodeId::from_static("retry"),
                NodeStanding::Paused { attempt: 2 },
            )
        },
        NodeFrame {
            attempt: Some(2),
            ..node_frame(
                &NodeId::from_static("scan"),
                NodeStanding::Reached(NodeState::Running { attempt: 2 }),
            )
        },
    ]
}

/// The status page of a run whose frame holds `nodes`, read at `Depth::Full`
/// where its trace is drawn.
fn trace_page(nodes: Vec<NodeFrame>) -> RunPage<'static> {
    let mut page = page_of(Vec::new(), Scope::Status);
    page.depth = Depth::Full;
    page.frame = Box::leak(Box::new(RunFrame {
        nodes,
        ..run_frame(&RUN)
    }));
    page
}

/// The line of `drawn` that names node `id` as a word of its own, or
/// nothing when no line does.
fn row_of(drawn: &str, id: &str) -> String {
    drawn
        .lines()
        .find(|line| line.split_whitespace().any(|word| word == id))
        .unwrap_or_default()
        .to_string()
}

/// The trace says beside each node what the run did with it, each fact
/// once: a gate's round, its reroutes and attempts, a wait on a person,
/// and what a node not yet run will do, where its failure goes said by
/// its arrow alone.
#[test]
fn the_trace_says_each_nodes_rounds_returns_and_waits() {
    let mut nodes = trace_nodes();
    nodes.extend(corrected_nodes());
    // A plain line is 80 cells, where the note column is 41 once the time
    // column is there: `ask`'s note, 59 cells, is cut short in it, so the
    // trace is drawn at the 120 cells a line may take.
    let look = Look {
        width: Width::of(None, Some(120)),
        ..Look::plain()
    };
    let drawn = draw(&trace_page(nodes), &look);
    let row = |id: &str| row_of(&drawn, id);

    let build = row("build");
    assert!(build.contains("rerouted twice"), "{build}");
    assert!(build.contains("3 attempts"), "{build}");
    let ask = row("ask");
    assert!(
        ask.contains("waited on you 12m04s over 2 rounds of questions"),
        "{ask}"
    );
    let approve = row("approve");
    assert!(approve.contains("1m00s"), "{approve}");
    assert!(approve.contains("its 2nd round"), "{approve}");
    assert!(!approve.contains("waited on you"), "{approve}");
    let lint = row("lint");
    assert!(lint.contains("lint <> fix-lint"), "{lint}");
    assert!(
        lint.contains("sends a failure back at most twice"),
        "{lint}"
    );
    assert!(!drawn.contains("sends its failure to"), "{drawn}");
    assert!(row("check").contains("a gate: you decide"), "{drawn}");
    let tidy = row("tidy");
    assert_eq!(tidy.matches("rerouted").count(), 1, "{tidy}");
    assert!(tidy.contains("exit 1 ; once"), "{tidy}");
    for id in ["retry", "scan"] {
        let on_attempt = row(id);
        assert!(on_attempt.contains("attempt 2"), "{on_attempt}");
        assert!(!on_attempt.contains("2 attempts"), "{on_attempt}");
    }
}

mod harvest;
mod words;
