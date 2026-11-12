//! Every place that offers a command to follow a run offers the one that does:
//! `yunta status <run> --follow`, by the handle a person reads and by the whole id
//! a program copies.

use super::*;

/// The command that follows the run `id`, spelled for a person.
fn follows(id: &str) -> String {
    format!("yunta status {} --follow", yunta_testkit::handle(id))
}

/// The line a detached run starts with, the answer a gate gives, and the
/// refusal of a run another engine drives: each one names a way to follow the
/// run, and that way is `--follow`.
///
/// Reached by: `crates/cli/src/commands/mod.rs`, `crates/cli/src/commands/resolve_gate.rs`,
/// `crates/cli/src/error.rs`
#[test]
fn every_place_that_says_follows_it_offers_follow() {
    let root = tempfile::tempdir().unwrap();
    let marks = root.path().join("marks");
    let checkout = holding(root.path(), &marks);
    let detached = yunta_in!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml", "--detach"]
    );
    let id = the_held_run(&checkout.home, &marks);
    let said = unwrapped(&format!(
        "{}{}",
        stdout(&detached),
        yunta_testkit::stderr(&detached)
    ));
    let started = format!("`{}` follows it", follows(&id));
    assert!(said.contains(&started), "no `{started}` in: {said}");

    let refused = yunta_in!(&checkout.repo, &checkout.home, &["resume", &id]);
    let said = unwrapped(&yunta_testkit::stderr(&refused));
    let moving = format!("{} follows it", follows(&id));
    assert!(said.contains(&moving), "no `{moving}` in: {said}");
    let refused = yunta_in!(&checkout.repo, &checkout.home, &["resume", &id, "--json"]);
    let document: serde_json::Value = serde_json::from_slice(&refused.stdout).unwrap();
    let whole = format!("yunta status {id} --follow");
    assert!(
        document["error"]["next"]
            .as_array()
            .is_some_and(|next| next.iter().any(|step| step["command"] == whole.as_str())),
        "{document:#}"
    );
    std::fs::write(marks.join("go"), "go").unwrap();

    let gated = tempfile::tempdir().unwrap();
    let (checkout, id) = ran(gated.path(), GATED);
    let answered = yunta_in!(&checkout.repo, &checkout.home, &["resolve-gate", &id, "go"]);
    let said = unwrapped(&stdout(&answered));
    let next = format!("{} follows it", follows(&id));
    assert!(said.contains(&next), "no `{next}` in: {said}");
}
