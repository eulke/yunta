//! What a decision says follows each of its answers, past what its label names.

use surco::glyphs::Glyphs;
use yunta_core::events::{
    AuthorGate, Claim, Escalation, GateOption, GateWaitingPayload, Leads, Menu,
};
use yunta_core::reserved::ReservedOption;
use yunta_core::{Because, LeftOut, NodeId, NonEmpty, OptionId, Workflow};

use crate::gate::{routes, Route};

const FLOW: &str = r#"
name: flow
nodes:
  - { id: plan, kind: bash, run: "true" }
  - { id: spec, kind: bash, run: "true", depends_on: [plan] }
  - { id: ok, kind: gate, assignee: lead, depends_on: [spec] }
  - { id: implement, kind: bash, run: "true", depends_on: [ok] }
  - { id: "review@claude", kind: bash, run: "true", depends_on: [implement] }
  - { id: "review@codex", kind: bash, run: "true", depends_on: [implement] }
  - { id: conform, kind: bash, run: "true", depends_on: [implement] }
  - { id: ship, kind: bash, run: "true", depends_on: ["review@claude", "review@codex", conform] }
"#;

fn flow() -> Workflow {
    yunta_core::yaml::parse(FLOW).expect("a workflow to read routes from")
}

/// A gate that goes on to `next` and back to `back`, or pauses.
fn gate(next: Vec<NodeId>, back: &'static str) -> GateWaitingPayload {
    Escalation::new(
        Claim::NeedsDecision {
            node: NodeId::from_static("ok"),
        },
        Menu::Gate(AuthorGate {
            assignee: "lead".to_string(),
            node: None,
        }),
        NonEmpty::from((
            GateOption::leading(
                OptionId::from_static("approve"),
                Leads::On {
                    next,
                    settles: false,
                },
            ),
            vec![
                GateOption::leading(
                    OptionId::from_static("adjust"),
                    Leads::Back {
                        node: NodeId::from_static(back),
                        resumes: true,
                    },
                ),
                GateOption::reserved(ReservedOption::Abort),
            ],
        )),
    )
    .into_payload()
}

fn asked() -> GateWaitingPayload {
    gate(vec![NodeId::from_static("implement")], "plan")
}

const APPROVE: OptionId = OptionId::from_static("approve");
const ADJUST: OptionId = OptionId::from_static("adjust");
const ABORT: OptionId = OptionId::from_static("abort");
const OK: NodeId = NodeId::from_static("ok");

/// Going on says what runs after the node its label names, what runs side by side
/// together, and a node run once per agent as one name and how many.
#[test]
fn going_on_says_what_runs_after_what_its_label_names() {
    let routes = routes(&asked(), &flow(), &[], Some(&OK));
    assert_eq!(
        routes[&APPROVE].said(Glyphs::Unicode),
        "review ×2, conform → ship"
    );
    assert_eq!(
        routes[&APPROVE].said(Glyphs::Ascii),
        "review x2, conform -> ship"
    );
}

/// Going back says what runs again after the node its label names, and that it comes
/// back to the person.
#[test]
fn going_back_says_what_runs_again_and_comes_back_to_the_person() {
    let routes = routes(&asked(), &flow(), &[], Some(&OK));
    assert_eq!(routes[&ADJUST].said(Glyphs::Unicode), "spec → ◆ ok");
    assert_eq!(routes[&ADJUST].said(Glyphs::Ascii), "spec -> ? ok");
}

/// What the run left out is not what follows.
#[test]
fn what_the_run_left_out_does_not_follow() {
    let gone = [LeftOut {
        node: NodeId::from_static("conform"),
        because: Because::Lacks { lacks: Vec::new() },
    }];
    let routes = routes(&asked(), &flow(), &gone, Some(&OK));
    assert_eq!(routes[&APPROVE].said(Glyphs::Unicode), "review ×2 → ship");
}

/// An option with no way on has no route: a pause, a gate nothing waits on, and going
/// back from a decision on no node.
#[test]
fn an_option_with_no_way_on_has_no_route() {
    let nothing = routes(&gate(Vec::new(), "plan"), &flow(), &[], Some(&OK));
    assert!(!nothing.contains_key(&ABORT));
    assert!(!nothing.contains_key(&APPROVE));
    let unnamed = routes(&asked(), &flow(), &[], None);
    assert!(!unnamed.contains_key(&ADJUST));
    assert!(unnamed.contains_key(&APPROVE));
}

/// An answer its label and words say all of has no route: going on to the last node,
/// and going back to the node right before the gate, which only asks again.
#[test]
fn an_answer_its_label_says_all_of_has_no_route() {
    let close = gate(vec![NodeId::from_static("ship")], "spec");
    let routes = routes(&close, &flow(), &[], Some(&OK));
    assert!(!routes.contains_key(&APPROVE), "{routes:?}");
    assert!(!routes.contains_key(&ADJUST), "{routes:?}");
}

/// The kinds of route compare by what they hold.
#[test]
fn a_route_holds_its_ranks() {
    let routes = routes(&asked(), &flow(), &[], Some(&OK));
    assert_eq!(
        routes[&ADJUST],
        Route::Back(vec![vec![NodeId::from_static("spec")], vec![OK]])
    );
}
