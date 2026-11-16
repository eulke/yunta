//! The past runs: two finished runs of `yunta/fragua`, installed as a pack in a repository
//! set up as the design's, each spending and taking its own.

use std::collections::BTreeSet;

use yunta_replay::frame::RunPhase;
use yunta_testkit_ui::small_runs::past;

use crate::{built_twice, stand};

#[tokio::test]
async fn two_finished_runs_of_fragua_each_spend_and_take_their_own() {
    let run = built_twice(past::runs).await;
    let workspace = run.workspace();
    let project = workspace.project().expect("the past runs' repository");
    let summaries = project.history(&"fragua".into()).await.summaries();
    assert_eq!(summaries.len(), 2, "{summaries:?}");
    let modes: BTreeSet<_> = summaries
        .iter()
        .map(|summary| summary.mode.as_str())
        .collect();
    assert_eq!(modes, BTreeSet::from(["quick"]));
    let spent: BTreeSet<_> = summaries.iter().map(|summary| summary.tokens).collect();
    let took: BTreeSet<_> = summaries.iter().map(|summary| summary.wall_clock).collect();
    assert_eq!((spent.len(), took.len()), (2, 2), "{summaries:?}");
    assert!(
        took.iter().all(Option::is_some),
        "each finished run took its time"
    );
    let pack = workspace
        .catalog_workflows()
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    assert!(pack.iter().any(|name| name == "yunta/fragua"), "{pack:?}");
    for summary in &summaries {
        let opened = project.run(&summary.run_id).await.expect("a past run");
        let manifest = &opened.manifest.doc;
        let from = manifest.pack.as_ref().map(|pack| pack.name.as_str());
        assert_eq!(
            from,
            Some("fragua"),
            "{} is a run of the pack",
            summary.run_id
        );
    }
    assert!(matches!(stand(&run).await.frame.phase, RunPhase::Finished));
}
