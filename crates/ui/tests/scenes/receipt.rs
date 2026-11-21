//! A finished run's receipt, through `State` alone, in what its frames cannot hold: showing
//! it has the run's event chain verified once, as its scene first stands, and asks for no
//! receipt written, so none stands beside the run after it; news of the run, and the person
//! pointing at a node and giving the lead back, ask for the verification no more.

use yunta_api::RunNews;
use yunta_replay::registry::Engine;
use yunta_testkit_ui::screens::run::{self, RunScreen};
use yunta_ui::prompt::Stroke;
use yunta_ui::{EvidenceOperation, Operation};

use super::{pane_drawn, scene_of};

/// How many times `screen`'s application asked its run's event chain verified, and how many
/// times its receipt written.
fn verified_and_written(screen: &RunScreen) -> (usize, usize) {
    let asked = |wanted: fn(&EvidenceOperation) -> bool| {
        let performed = screen.performed().iter();
        performed
            .filter(|operation| matches!(operation, Operation::Evidence(work) if wanted(work)))
            .count()
    };
    (
        asked(|work| matches!(work, EvidenceOperation::Verify { .. })),
        asked(|work| matches!(work, EvidenceOperation::WriteReceipt { .. })),
    )
}

#[tokio::test]
async fn showing_the_receipt_verifies_its_chain_once_and_writes_no_receipt() {
    let mut screen = run::opened("receipt").await;
    assert_eq!(scene_of(&screen), Some("receipt"));
    assert_eq!(verified_and_written(&screen), (1, 0));
    let pane = pane_drawn(&screen, 84, 26);
    assert!(
        pane.iter().any(|row| row == "  + event chain  intact"),
        "the chain the store verified stands in the receipt: {pane:#?}"
    );
    // The run's registry read again: news that asks for nothing, and drives nothing.
    screen.handed(RunNews::Engine(Engine::Unrecorded));
    screen.pressed(&[]).await;
    // The person points at a node, then gives the lead back: the receipt stands again.
    screen.pressed(&[Stroke::Insert('j')]).await;
    assert_eq!(scene_of(&screen), Some("the node's own"));
    screen.pressed(&[Stroke::Insert('f')]).await;
    assert_eq!(scene_of(&screen), Some("receipt"));
    assert_eq!(
        verified_and_written(&screen),
        (1, 0),
        "the chain is verified once for the run, and no receipt is asked written"
    );
    let written = screen.run_dir().join("receipt.md");
    assert!(!written.exists(), "{} stands", written.display());
}
