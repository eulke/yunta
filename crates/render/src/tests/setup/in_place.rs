//! See [`super`]. A setup's rows as they change: a role changed in place, its adapters with
//! one chosen over its model's box, as its frames draw it; and the pack installed already,
//! said so, the row that writes installing nothing.

use surco::glyphs::Glyphs;
use yunta_core::{AdapterId, PackRef};

use super::{answering, at, fraguas_roles, frame, on, probed, rows, rows_of, rust_on_github};
use crate::page::{
    Candidates, Choosing, CommitsAs, OfferedPack, Pointed, ProposedRole, RepositoryPast, Setup,
    Typing,
};

/// A role changed in place: its adapters with the first chosen where its candidates stood,
/// its model's box under them and what an empty one comes to, as `setup-changing` draws it at
/// each width; the pointer on the role, none on the row that writes.
#[test]
fn a_role_changed_in_place_says_the_rows_its_frames_carry() {
    let (detected, adapters, pack) = (
        rust_on_github(),
        answering(),
        "yunta/fragua".parse::<PackRef>().expect("a pack"),
    );
    let runners = fraguas_roles();
    let roles: Vec<ProposedRole<'_>> = runners.iter().map(|runner| on(runner, &adapters)).collect();
    let probed = [probed("claude-code", true), probed("codex", true)];
    let offered: Vec<&AdapterId> = adapters.iter().collect();
    let setup = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "yunta",
        base_branch: "main",
        detected: &detected,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: Some(OfferedPack::InReach(&pack)),
        probed: &probed,
        roles: &roles,
        pointed: Some(Pointed::Role(
            &runners[4],
            Choosing::among(&offered, 0, Typing::default(), false),
        )),
    };
    assert_eq!(
        rows(&setup, at(92, Glyphs::Unicode), Candidates::Every),
        rows_of(&frame("setup-changing.unicode-120"), (8, 29), 14)
    );
    assert_eq!(
        rows(&setup, at(92, Glyphs::Ascii), Candidates::Every),
        rows_of(&frame("setup-changing.ascii-120"), (8, 29), 14)
    );
    let narrow = rows(&setup, at(78, Glyphs::Unicode), Candidates::First);
    let window = rows_of(&frame("setup-changing.unicode-80"), (11, 22), 1);
    assert_eq!(narrow[narrow.len() - window.len()..], window[..]);
}

/// A pack the repository vendors already is said installed, not out of reach: its row says
/// so, and the row that writes writes the configuration alone.
#[test]
fn with_the_pack_installed_already_the_workflow_row_says_so_and_the_write_row_installs_nothing() {
    let (detected, adapters, pack) = (
        rust_on_github(),
        answering(),
        "yunta/fragua".parse::<PackRef>().expect("a pack"),
    );
    let runners = fraguas_roles();
    let roles: Vec<ProposedRole<'_>> = runners.iter().map(|runner| on(runner, &adapters)).collect();
    let probed = [probed("claude-code", true), probed("codex", true)];
    let setup = Setup {
        past: Some(RepositoryPast::NeverUsed),
        name: "yunta",
        base_branch: "main",
        detected: &detected,
        committer: CommitsAs::Named("Marta Ruiz"),
        pack: Some(OfferedPack::Installed(&pack)),
        probed: &probed,
        roles: &roles,
        pointed: Some(Pointed::Write),
    };
    assert_eq!(setup.writes(), "write .yunta/config.yaml");
    let drawn = rows(&setup, at(92, Glyphs::Unicode), Candidates::Every);
    let workflow = drawn.iter().find(|row| row.contains("workflow"));
    let said = Some("  ✓ workflow      yunta/fragua, installed as a pack");
    assert_eq!(workflow.map(String::as_str), said, "{}", drawn.join("\n"));
    let write = drawn.last().map(String::as_str);
    assert_eq!(
        write,
        Some(" ▸ write .yunta/config.yaml  · then yunta asks what to build")
    );
}
