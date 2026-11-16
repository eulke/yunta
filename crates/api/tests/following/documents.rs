//! What a followed run's nodes hand over, and what its manifest holds that this binary
//! does not read: each document read once, as it is handed over or with the run read
//! whole, one that fails today's shape said unread in its place; the manifest's unknown
//! keys said once for each run read. Over runs a bench drives.

use std::path::Path;

use yunta_api::{Document, Followed, FollowedRun, Following, Remark, RunNews};
use yunta_core::events::{
    ArtifactAcceptedPayload, ArtifactEvent, ArtifactId, EventDraft, EventPayload,
};
use yunta_core::shown::ShownContent;
use yunta_core::{ArtifactKind, NodeId};
use yunta_testkit_core::FixedClock;

use super::awaited::{broken_spec, within, SPECIFIED, SPECIFYING};
use super::{whole, World, NOTHING_COMES};

/// The plan's one task, as its planner titles it.
const TITLED: &str = "Write the greeting";

/// The document of `kind` that `node` of the run of `world` handed over, accepted again
/// as the bytes `rewrite` makes of it, under their own hash, as a node that hands it over
/// again does.
pub(super) fn accepted_again(
    world: &World,
    (node, kind): (&str, ArtifactKind),
    rewrite: impl FnOnce(Vec<u8>) -> Vec<u8>,
) {
    let run = &world.bench.run_id;
    let artifact = ArtifactId::Interpreted { kind };
    let accepted = world
        .logged(run)
        .iter()
        .find_map(|event| match event.payload() {
            Some(EventPayload::Artifacts(ArtifactEvent::Accepted(accepted)))
                if accepted.artifact == artifact =>
            {
                Some(accepted.clone())
            }
            _ => None,
        })
        .expect("the run accepted the document");
    let dir = world.bench.run_dir();
    let held = std::fs::read(yunta_core::run_dir::object(&dir, &accepted.content_hash)).unwrap();
    let bytes = rewrite(held);
    let hash = yunta_core::sha256_hex(&bytes);
    std::fs::write(yunta_core::run_dir::object(&dir, &hash), bytes).unwrap();
    let again = ArtifactAcceptedPayload {
        content_hash: hash,
        ..accepted
    };
    let payload = EventPayload::Artifacts(ArtifactEvent::Accepted(again));
    let draft = EventDraft::new(run.clone(), Some(NodeId::from(node)), payload).unwrap();
    world.bench.storage.append(&draft, &FixedClock).unwrap();
}

/// Each document handed over among what `following` hands within the time nothing more
/// comes, with the node that handed it over, each news folded into `kept`.
async fn handed(following: &mut Following, kept: &mut FollowedRun) -> Vec<(String, Document)> {
    let came = within(following, kept, NOTHING_COMES).await;
    came.into_iter()
        .filter_map(|news| match news {
            RunNews::Document { node, document } => Some((node.to_string(), document)),
            _ => None,
        })
        .collect()
}

/// The nodes `handed` names, in its order.
fn nodes(handed: &[(String, Document)]) -> Vec<&str> {
    handed.iter().map(|(node, _)| node.as_str()).collect()
}

/// The titles of the tasks of the plan `document` holds, read.
fn titles(document: &Document) -> Vec<&str> {
    let Document::Read(read) = document else {
        panic!("the plan is read, and came {document:?}");
    };
    let ShownContent::Tasks(review) = &read.content else {
        panic!("a plan, and came {read:?}");
    };
    review
        .plan
        .tasks
        .iter()
        .map(|task| task.title.as_str())
        .collect()
}

/// Each document the run's nodes handed over before it was read whole comes with it,
/// once, the plan with its tasks' titles; one handed over while it is followed comes
/// once it is folded; no further look hands either again.
#[tokio::test]
async fn each_document_a_node_hands_over_is_handed_once() {
    let world = World::new();
    world.bench.run(SPECIFIED, SPECIFYING).await;
    let run = world.bench.run_id.clone();
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let came = handed(&mut following, &mut kept).await;
    assert_eq!(nodes(&came), ["plan", "spec"], "each one once: {came:?}");
    assert_eq!(titles(&came[0].1), [TITLED]);

    let farewell = |bytes: Vec<u8>| {
        let text = String::from_utf8(bytes).unwrap();
        text.replace(TITLED, "Write the farewell").into_bytes()
    };
    accepted_again(&world, ("plan", ArtifactKind::Tasks), farewell);
    let came = handed(&mut following, &mut kept).await;
    assert_eq!(
        nodes(&came),
        ["plan"],
        "the plan handed over again: {came:?}"
    );
    assert_eq!(titles(&came[0].1), ["Write the farewell"]);
}

/// A spec whose bytes fail today's shape is said unread in its place, with where a person
/// opens it and why; the plan beside it is read.
#[tokio::test]
async fn a_handed_document_that_fails_its_shape_arrives_unread() {
    let world = World::new();
    world.bench.run(SPECIFIED, SPECIFYING).await;
    let run = world.bench.run_id.clone();
    broken_spec(&world);
    let mut following = world.follow(&run).await;
    let (mut kept, _) = whole(&mut following).await;

    let came = handed(&mut following, &mut kept).await;
    assert_eq!(nodes(&came), ["plan", "spec"], "each one once: {came:?}");
    assert_eq!(titles(&came[0].1), [TITLED], "the rest are read");
    let Document::Unread { shown, path, why } = &came[1].1 else {
        panic!(
            "the spec that fails its shape is unread, and came {:?}",
            came[1]
        );
    };
    assert_eq!(shown.producer.as_ref().map(NodeId::as_str), Some("spec"));
    assert!(path.ends_with("spec.yaml"), "{}", path.display());
    assert!(why.contains("`specs`"), "why names where it fails: {why}");
}

/// A key a newer binary wrote into the manifest at `dir`, which this one does not read.
fn newer_key_in(dir: &Path) {
    let path = yunta_engine::run_dir::manifest_path(dir);
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("something_later: 7\n");
    std::fs::write(path, text).unwrap();
}

/// What `came` remarks on the runs' manifests.
fn remarked(came: &[RunNews]) -> Vec<Vec<String>> {
    came.iter()
        .filter_map(|news| match news {
            RunNews::Remark(Remark::UnreadManifestKeys { keys }) => Some(keys.clone()),
            _ => None,
        })
        .collect()
}

/// A manifest's unknown keys are said once for the run read whole, whatever the looks
/// after, and once again for each run read whole after it.
#[tokio::test]
async fn a_manifests_unknown_keys_are_said_once_for_each_run_read() {
    let world = World::new();
    let (first, dir) = world.born("first").await;
    newer_key_in(&dir);
    let (second, dir) = world.born("second").await;
    newer_key_in(&dir);
    let unread = vec![vec!["something_later".to_string()]];
    let mut following = world.follow(&first).await;
    let (mut kept, _) = whole(&mut following).await;
    let came = within(&mut following, &mut kept, NOTHING_COMES).await;
    assert_eq!(
        remarked(&came),
        unread,
        "said with the run read whole: {came:?}"
    );

    world.slept(&first);
    let came = within(&mut following, &mut kept, NOTHING_COMES).await;
    assert!(remarked(&came).is_empty(), "never at a look: {came:?}");

    following.refollow(Followed {
        field: None,
        run: Some(second),
    });
    let (mut kept, _) = whole(&mut following).await;
    let came = within(&mut following, &mut kept, NOTHING_COMES).await;
    assert_eq!(
        remarked(&came),
        unread,
        "said for the next run read: {came:?}"
    );
}
