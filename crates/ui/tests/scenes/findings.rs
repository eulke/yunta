//! The findings scene of the design's run, through `State` alone: as its reviewers post their
//! findings, the pane counts them answered of posted and lists each a row, the run's finding
//! pointer on the first no node answered; `j k` move that pointer, which the person then
//! holds as more are answered; `tab` hands `j k` to the nodes, and the legend says which
//! they move.

use surco::components::Keys;
use surco::doc::Block;
use surco::look::Look;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;

/// Where the design's run stands once `review@reviewer` posted the first finding.
const FIRST_POSTED: u64 = 191;

/// Once `review@reviewer-alt` posted its second, the run's third.
const THIRD_POSTED: u64 = 196;

/// Once `conform` posted its blocking one, the run's fourth and last.
const FOURTH_POSTED: u64 = 199;

/// Once `fix-findings` answered the last of the four.
const ALL_ANSWERED: u64 = 229;

/// What `screen`'s pane says of the run's findings, in plain glyphs: the heading it is
/// titled by, how many rows its list holds, and the row the finding pointer stands on. None
/// where the pane lists none.
fn listed(screen: &RunScreen) -> Option<(String, usize, Option<usize>)> {
    let main = screen.state().screen(&Look::plain()).main;
    let heading = main.title.first()?.clone();
    let list = main.doc.blocks().iter().find_map(|block| match block {
        Block::Findings(list) => Some(list),
        _ => None,
    })?;
    Some((heading, list.rows.len(), list.pointed))
}

/// What `screen`'s legend says `j k` move.
fn j_and_k_move(screen: &RunScreen) -> Option<String> {
    let legend = screen.state().screen(&Look::plain()).legend;
    legend.0.iter().find_map(|binding| match &binding.keys {
        Keys::Typed(keys) if keys == "j k" => Some(binding.does.clone()),
        _ => None,
    })
}

/// The node the trail names after the run's handle.
fn named(screen: &RunScreen) -> Option<String> {
    let trail = screen.state().screen(&Look::plain()).trail;
    trail.get(2).cloned()
}

#[tokio::test]
async fn as_reviewers_post_each_finding_stands_a_row_counted_with_the_pointer_on_the_first_open() {
    let mut screen = run::opened("board-moved").await;
    let posted = [
        (
            FIRST_POSTED,
            "findings ; 0 of 1 answered ; review@reviewer",
            1,
        ),
        (
            THIRD_POSTED,
            "findings ; 0 of 3 answered ; review@reviewer, review@reviewer-alt",
            3,
        ),
        (
            FOURTH_POSTED,
            "findings ; 0 of 4 answered ; review@reviewer, review@reviewer-alt, conform",
            4,
        ),
    ];
    for (seq, heading, rows) in posted {
        screen.went_on(seq).await;
        assert_eq!(
            listed(&screen),
            Some((heading.to_string(), rows, Some(0))),
            "at {seq}"
        );
    }
}

#[tokio::test]
async fn tab_hands_j_and_k_to_the_nodes_and_the_legend_says_so() {
    let mut screen = run::opened("findings").await;
    assert_eq!(j_and_k_move(&screen).as_deref(), Some("finding"));
    screen.pressed(&[Stroke::Tab]).await;
    assert_eq!(j_and_k_move(&screen).as_deref(), Some("node"));
    screen.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(named(&screen).as_deref(), Some("review@reviewer-alt"));
    screen.pressed(&[Stroke::Tab]).await;
    assert_eq!(j_and_k_move(&screen).as_deref(), Some("finding"));
}

#[tokio::test]
async fn the_finding_pointer_the_person_holds_stays_on_an_answered_finding_as_more_are_answered() {
    // Two of four answered: the run points at the first still open.
    let mut screen = run::opened("findings-answered").await;
    let pointed = |screen: &RunScreen| listed(screen).and_then(|(_, _, pointed)| pointed);
    assert_eq!(pointed(&screen), Some(1));
    screen.pressed(&[Stroke::Insert('k')]).await;
    assert_eq!(
        pointed(&screen),
        Some(0),
        "`k` points at the one answered above"
    );
    screen.went_on(ALL_ANSWERED).await;
    let (heading, _, at) = listed(&screen).expect("the pane lists the findings");
    assert!(heading.contains("4 of 4 answered"), "{heading}");
    assert_eq!(at, Some(0), "the person's pointer stays where they left it");
}
