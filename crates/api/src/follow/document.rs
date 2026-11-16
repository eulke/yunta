//! See [`super`]. What the nodes of the run read whole hand over, and what reading the
//! run remarks on.
//!
//! Each document a node hands over that a person reads whole, a plan, a spec or a
//! review's findings, is read once: those the run holds as it is read whole are handed
//! after it, and each one after that once a look folded the event that hands it over.
//! Which they are is worked out again only once the run held moved past the head it was
//! last worked out at, from what its state holds past that head, so a run that stands
//! still costs nothing. Each is read on its own, through the engine's reading of a
//! node's documents, so one that fails today's shape is handed unread in its place, with
//! where a person opens it and why, and the rest are read. A document is owed until it is
//! read and kept to hand, so a look dropped half way leaves it to the next; a refollow
//! that lets the run go forgets what was owed of it, and a run held again is handed again
//! what it handed over.
//!
//! The keys of a run's manifest that this binary does not read are said once for each
//! run read whole, as `yunta status` says them, before any of its documents: read off the
//! manifest the run froze as the run is newly held, never at a look.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use yunta_core::events::Shown;
use yunta_core::shown::ShownDocument;
use yunta_core::{NodeId, RunId, Seq};

use super::{Following, RunNews};
use crate::run::{read_manifest, unread_keys};
use crate::{Error, Remark};

/// A document a node of the run handed over, as the follower read it. Not
/// [`ShownDocument`], what a decision shows.
#[derive(Debug, Clone)]
pub enum Document {
    /// Read, with what it says.
    Read(ShownDocument),
    /// The document `shown` names, which a person opens at `path`, not read, for `why`:
    /// it fails today's shape, or its bytes cannot be read.
    Unread {
        shown: Shown,
        path: PathBuf,
        why: String,
    },
}

/// What the run held handed over, as far as the following worked it out and handed it.
/// Not [`Document`], what is handed of one.
pub(super) struct Handing {
    /// The run and the head what it handed over was last worked out at.
    worked_out_at: Option<(RunId, Option<Seq>)>,
    /// Each document handed over that is not read and kept to hand yet, with the node
    /// that handed it over, in the order the run accepted them.
    owed: VecDeque<(NodeId, Shown)>,
}

impl Handing {
    /// Nothing worked out yet.
    pub(super) fn new() -> Handing {
        Handing {
            worked_out_at: None,
            owed: VecDeque::new(),
        }
    }

    /// What was worked out of the run a refollow lets go, forgotten with what was owed of
    /// it: a run held again is handed again what it handed over, and what reading it
    /// remarks on, as one held for the first time is.
    pub(super) fn forget(&mut self) {
        self.worked_out_at = None;
        self.owed.clear();
    }
}

impl Following {
    /// What the run held handed over since it was last worked out, each document read
    /// and handed, after what reading it remarks on when it is newly held. A read that
    /// fails is handed unread, and the following goes on.
    pub(super) async fn hand_over(&mut self) {
        self.work_out_handed_over();
        while let Some(document) = self.read_owed().await {
            self.handing.owed.pop_front();
            self.found(document);
        }
    }

    /// The documents the run held handed over past the head they were last worked out
    /// at, owed; and, for a run newly held, what was owed of the run before it dropped and
    /// its manifest's unknown keys said.
    fn work_out_handed_over(&mut self) {
        let Some(held) = &self.held else {
            return;
        };
        let at = (held.run.run.clone(), held.run.state.head());
        let (newly_held, past) = match &self.handing.worked_out_at {
            Some((run, head)) if *run == at.0 => (false, *head),
            _ => (true, None),
        };
        if !newly_held && past == at.1 {
            return;
        }
        if newly_held {
            self.handing.owed.clear();
        }
        let documents = yunta_engine::documents_handed_over(&held.run.state, past);
        self.handing.owed.extend(documents);
        let remark = newly_held
            .then(|| unread_keys_in(held.registry.run_dir()))
            .flatten();
        self.handing.worked_out_at = Some(at);
        if let Some(remark) = remark {
            self.found(RunNews::Remark(remark));
        }
    }

    /// The first document owed, read from the run held as the following holds it: none
    /// while none is owed.
    async fn read_owed(&self) -> Option<RunNews> {
        let held = self.held.as_ref()?;
        let (node, shown) = self.handing.owed.front()?;
        let dir = held.registry.run_dir();
        let asked = held.run.manifest.request();
        let read = yunta_engine::produced_document(dir, shown, &held.run.state, asked).await;
        let document = match read {
            Ok(read) => Document::Read(read),
            Err(error) => Document::Unread {
                shown: shown.clone(),
                path: dir.join(yunta_core::run_dir::view_of(shown)),
                why: yunta_core::describe(&Error::from(error)),
            },
        };
        let node = node.clone();
        Some(RunNews::Document { node, document })
    }
}

/// The remark that says the keys of the manifest the run in `dir` froze that this binary
/// does not read: none where it holds none. A manifest that no longer reads owes no
/// remark: the run was read whole from it, and a page of the run says what stops it
/// reading.
fn unread_keys_in(dir: &Path) -> Option<Remark> {
    let manifest = read_manifest(&yunta_engine::run_dir::manifest_path(dir)).ok()?;
    unread_keys(&manifest)
}
