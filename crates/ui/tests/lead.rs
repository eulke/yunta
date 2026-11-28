//! Who leads a run's screen, through `State` alone, on the cuts of the design's targets:
//! while the run leads, its pointer stands on the node it waits on a person at, else on the
//! first node at work in the workflow's declared order, else on the last node that ran,
//! and goes with the run as its news come; `j` takes it for the person, once `tab` handed
//! `j k` to the nodes where the pane lists findings, the run's news leave it there, and `f`
//! gives it back. `esc` on a prompt in place hands the person the node it waits at, and `f`
//! gives it back with the prompt. The trail names the node pointed at.

use surco::components::Keys;
use surco::glyphs::Glyphs;
use surco::look::{Look, Width};
use surco::motion::Motion;
use surco::paint::Paint;
use surco::surface::Terminal;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::Scene;

/// The cells the region beside a run's page takes on a terminal from 102 columns: a run's
/// nodes, at its left.
const SIDE: usize = 31;

/// Where the design's run stands once `conform`, the last of its three reviewers at work,
/// finished, the other two still at work.
const CONFORM_FINISHED: u64 = 204;

/// Where it stands once both other reviewers finished and `fix-findings` started.
const FIX_FINDINGS_STARTED: u64 = 213;

/// The node the trail names after the run's handle, where it names one.
fn named(screen: &RunScreen) -> Option<String> {
    let trail = screen.state().screen(&Look::plain()).trail;
    let [_, _, node] = trail.as_slice() else {
        return None;
    };
    Some(node.clone())
}

/// The node whose row the pointer stands on beside the run's page, as a terminal draws the
/// row: its name among the row's words.
fn pointed(screen: &RunScreen) -> Option<String> {
    let look = Look {
        glyphs: Glyphs::Unicode,
        paint: Paint::Plain,
        width: Width::exact(SIDE),
        motion: Motion::Still,
    };
    let side = screen.state().screen(&look).side?;
    let rows = Terminal::on(look).lines(&side.region.doc);
    let row = rows
        .iter()
        .map(|line| line.text())
        .find(|row| row.trim_start().starts_with(Glyphs::Unicode.pointer()))?;
    let names = screen
        .state()
        .followed_runs()
        .in_view
        .as_ref()
        .map(|in_view| {
            in_view
                .run
                .manifest()
                .workflow
                .iter_nodes()
                .map(|node| node.id.to_string())
        })?
        .collect::<Vec<_>>();
    row.split_whitespace()
        .find(|word| names.iter().any(|name| name == word))
        .map(str::to_string)
}

/// Where the pointer stands on `screen`: the node its row and the trail both name.
fn stands_on(screen: &RunScreen) -> (Option<String>, Option<String>) {
    (pointed(screen), named(screen))
}

/// `node`, as both the pointer's row and the trail name it.
fn on(node: &str) -> (Option<String>, Option<String>) {
    (Some(node.to_string()), Some(node.to_string()))
}

/// Whether the legend offers `f`, which gives the lead back: only while the run does not
/// lead.
fn offers_the_lead_back(screen: &RunScreen) -> bool {
    let legend = screen.state().screen(&Look::plain()).legend;
    legend
        .0
        .iter()
        .any(|binding| matches!(&binding.keys, Keys::Typed(key) if key == "f"))
}

#[tokio::test]
async fn the_run_leads_its_pointer_until_a_person_takes_it_and_f_gives_it_back() {
    // Three reviewers at work: the first of them in declared order.
    let mut screen = run::opened("findings").await;
    assert_eq!(
        stands_on(&screen),
        on("review@reviewer"),
        "the first at work"
    );
    assert!(!offers_the_lead_back(&screen), "the run leads");
    screen.went_on(CONFORM_FINISHED).await;
    assert_eq!(
        stands_on(&screen),
        on("review@reviewer"),
        "kept there as `conform` moves and ends"
    );

    // The pane lists the findings, which `j k` move along: `tab` hands them to the nodes.
    screen.pressed(&[Stroke::Tab, Stroke::Insert('j')]).await;
    assert_eq!(
        stands_on(&screen),
        on("review@reviewer-alt"),
        "`j` takes it"
    );
    assert!(offers_the_lead_back(&screen), "the person leads");
    screen.went_on(FIX_FINDINGS_STARTED).await;
    assert_eq!(
        stands_on(&screen),
        on("review@reviewer-alt"),
        "the run's news leave the person's pointer where they put it"
    );

    screen.pressed(&[Stroke::Insert('f')]).await;
    assert_eq!(
        stands_on(&screen),
        on("fix-findings"),
        "`f` gives it back to the node at work"
    );
    assert!(!offers_the_lead_back(&screen), "the run leads again");
}

#[tokio::test]
async fn esc_on_a_card_hands_its_node_to_the_person_until_f_gives_it_back_with_the_card() {
    let mut screen = run::opened("scope").await;
    assert!(
        !offers_the_lead_back(&screen),
        "the run leads with its card"
    );
    screen.pressed(&[Stroke::Decline]).await;
    assert_eq!(
        stands_on(&screen),
        on("implement"),
        "the node the card waits at"
    );
    assert!(offers_the_lead_back(&screen), "the person leads");
    screen.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(stands_on(&screen), on("review@reviewer"), "the person's");

    screen.pressed(&[Stroke::Insert('f')]).await;
    assert_eq!(
        stands_on(&screen),
        on("implement"),
        "`f` gives it back to the node the run waits at"
    );
    assert!(!offers_the_lead_back(&screen), "the run leads again");
    assert!(
        matches!(screen.state().scene(), Some(Scene::Card(..))),
        "its card in place again: {:?}",
        screen.state().scene()
    );
}

#[tokio::test]
async fn the_pointer_stands_on_the_node_the_run_waits_on_a_person_at() {
    // The plan's gate, which the log has not started: the node the pause names, which the
    // trail names; the plan's desk stands in the place of the nodes.
    let screen = run::opened("desk").await;
    assert_eq!(stands_on(&screen), (None, Some("approve-plan".to_string())));
    // A gate an older log parked on an escalation with no menu, which never started: the
    // node the escalation says it is about, not the one that ran last.
    let screen = run::opened("standstill").await;
    assert_eq!(stands_on(&screen), on("approve"));
}

#[tokio::test]
async fn the_pointer_stands_on_the_last_node_that_ran_where_none_works_or_waits() {
    // The suite failed and the run stands at its failure's pause, which names no node.
    let screen = run::opened("failure").await;
    assert_eq!(stands_on(&screen), on("tests"));
    // A run held for a limit's reset: the node it holds is the last that ran.
    let screen = run::opened("wakes-alone").await;
    assert_eq!(stands_on(&screen), on("pr"));
}
