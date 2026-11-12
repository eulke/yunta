//! The packs installed, as a person moves through them, in every environment: the
//! rows `pack list` prints, the pointer on one.

use std::path::{Path, PathBuf};

use surco::golden::{assert_golden, ENVIRONMENTS};
use surco::look::Look;
use surco::surface::{Markdown, Surface, Terminal};
use yunta_core::{PackLockEntry, PackRef};

use crate::page::pack::{Listed, PointedPacks, VendoredPack, VendoredTree};
use crate::page::Page;

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/packs")
}

/// The pack `reference`, locked at `v1.0.0`, its tree standing as `tree` says.
fn vendored(reference: &str, name: &str, tree: VendoredTree) -> VendoredPack {
    let entry = format!(
        "publisher: acme\nname: {name}\nsource: /home/me/packs/{name}\nref: v1.0.0\n\
         commit: 3f9c2e1a0b4d5c6e7f8091a2b3c4d5e6f7a8b9c0\n\
         content_hash: 0000000000000000000000000000000000000000000000000000000000000000\n"
    );
    VendoredPack {
        pack: reference.parse::<PackRef>().expect("a pack reference"),
        locked: yunta_core::yaml::parse::<PackLockEntry>(&entry).expect("a lock entry"),
        tree,
    }
}

/// Two packs installed, one as its lock records it and one modified since.
fn listed() -> Listed {
    Listed::Locked {
        packs: vec![
            vendored("acme/lint", "lint", VendoredTree::AsLocked),
            vendored("acme/review", "review", VendoredTree::Modified),
        ],
        lock: ".yunta/yunta.lock".to_string(),
    }
}

#[test]
fn the_packs_pointed_at_match_their_goldens_in_every_environment() {
    let listed = listed();
    let pointed = "acme/review".parse::<PackRef>().expect("a pack reference");
    let page = PointedPacks {
        listed: &listed,
        pointed: &pointed,
    };
    for environment in &ENVIRONMENTS {
        let look = Look::of(environment);
        let drawn = Terminal::on(look).draw(&page.document(&look));
        assert_golden(&environment.golden(&goldens(), "pointed"), &drawn);
    }
}

/// The packs `pack list` prints carry no pointer.
#[test]
fn the_packs_listed_whole_carry_no_pointer() {
    let drawn = Markdown.draw(&listed().document(&Markdown.look()));
    assert!(!drawn.contains('▸'), "{drawn}");
}
