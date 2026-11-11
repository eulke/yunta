//! See [`super`]. A pack installed or updated from the packs: where it is, or the
//! ref the installed one moves to, asked ([`PackAsked`]); the pack fetched, its copy
//! held under the fetch's ticket and its inventory read; then installed from that
//! very copy, a confirmation naming what it executes first where that waits on a
//! person; and what installing came to.

use std::sync::Arc;

use surco::components::{Legend, Note};
use surco::doc::Doc;
use surco::look::Look;
use yunta_core::PackRef;
use yunta_render::page::pack::{Installed, Inventory, Needs, Updated};
use yunta_render::page::Page;

use super::{Next, PacksShown, PacksView};
use crate::dialog::Confirmation;
use crate::prompt::{Answerable, LineAsked, Press, Stroke};
use crate::{Operation, PackFetched, PackInstalled, PackOperation};

/// Where a pack to install is, or the ref an installed one moves to, being typed.
#[derive(Debug, Clone)]
pub(super) enum PackAsked {
    /// Where a pack new to the repository is.
    Source(LineAsked),
    /// The ref the installed `pack` moves to.
    Reference { pack: PackRef, line: LineAsked },
}

impl PackAsked {
    /// Where a pack to install is, asked.
    pub(super) fn source() -> Self {
        let asks = yunta_render::screen::pack_source_asks();
        PackAsked::Source(LineAsked::new(asks))
    }

    /// The ref the installed `pack` moves to, asked.
    pub(super) fn reference(pack: PackRef) -> Self {
        let asks = yunta_render::screen::pack_reference_asks(&pack.to_string());
        PackAsked::Reference {
            pack,
            line: LineAsked::new(asks),
        }
    }

    /// The line it is typed on.
    pub(super) fn line(&self) -> &LineAsked {
        match self {
            PackAsked::Source(line) | PackAsked::Reference { line, .. } => line,
        }
    }

    /// Where the trail goes on past the packs: an install, or the pack updated.
    pub(super) fn trail(&self) -> String {
        match self {
            PackAsked::Source(_) => yunta_render::screen::INSTALL.to_string(),
            PackAsked::Reference { pack, .. } => pack.to_string(),
        }
    }

    /// What `stroke` does to the line: answered, the fetch it asks for and what is
    /// fetched as a person reads it; or what the line makes of it otherwise.
    fn pressed(&mut self, stroke: Stroke) -> Press<(PackOperation, String)> {
        let line = match self {
            PackAsked::Source(line) | PackAsked::Reference { line, .. } => line,
        };
        match line.press(stroke) {
            Press::Answer(typed) => Press::Answer(self.fetch(typed.value)),
            Press::Stay => Press::Stay,
            Press::Moved => Press::Moved,
            Press::Decline => Press::Decline,
            Press::Interrupt => Press::Interrupt,
        }
    }

    /// The fetch `typed` answers the question with, and what it fetches as a person
    /// reads it: the source typed, or the pack updated.
    fn fetch(&self, typed: String) -> (PackOperation, String) {
        match self {
            PackAsked::Source(_) => (
                PackOperation::Fetch {
                    source: typed.clone(),
                },
                typed,
            ),
            PackAsked::Reference { pack, .. } => (
                PackOperation::FetchUpdate {
                    pack: pack.clone(),
                    reference: typed,
                },
                pack.to_string(),
            ),
        }
    }
}

impl PacksView {
    /// What `stroke` does while a pack is asked for, fetched or installed.
    pub(super) fn pressed_installing(&mut self, stroke: Stroke) -> Next {
        match &mut self.shown {
            PacksShown::Asking(asked) => match asked.pressed(stroke) {
                Press::Answer((work, what)) => self.fetching(work, what),
                other => self.left(other),
            },
            PacksShown::Fetching { work, .. } => match stroke {
                Stroke::Decline => {
                    let fetch = Operation::Pack(work.clone());
                    self.shown = PacksShown::Listing;
                    Next::LetGo(fetch)
                }
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            PacksShown::Fetched(fetched) => match stroke {
                Stroke::Enter => {
                    let fetched = Arc::clone(fetched);
                    self.install(&fetched)
                }
                Stroke::Decline => {
                    let held = fetched.held;
                    self.shown = PacksShown::Listing;
                    Next::Release(held)
                }
                Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            PacksShown::Installing { .. } => match stroke {
                Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            PacksShown::Installed(_) | PacksShown::Updated { .. } => match stroke {
                Stroke::Decline | Stroke::Enter => {
                    self.shown = PacksShown::Listing;
                    self.remarked.clear();
                    Next::Work(Operation::Pack(PackOperation::List))
                }
                Stroke::Insert('q') | Stroke::Interrupt => Next::Leave,
                _ => Next::Stays,
            },
            PacksShown::Listing
            | PacksShown::Auditing { .. }
            | PacksShown::Audited { .. }
            | PacksShown::Naming(_)
            | PacksShown::Scaffolding { .. }
            | PacksShown::Scaffolded(_) => Next::Stays,
        }
    }

    /// The pack `work` fetches, named `what`, asked for.
    fn fetching(&mut self, work: PackOperation, what: String) -> Next {
        let fetch = Operation::Pack(work.clone());
        self.shown = PacksShown::Fetching { work, what };
        self.remarked.clear();
        Next::Work(fetch)
    }

    /// What a question's `press` asks where it did not answer: back to the listing
    /// for one left, and leaving for Ctrl-C.
    fn left<T>(&mut self, press: Press<T>) -> Next {
        match press {
            Press::Decline => {
                self.shown = PacksShown::Listing;
                Next::Stays
            }
            Press::Interrupt => Next::Leave,
            Press::Answer(_) | Press::Stay | Press::Moved => Next::Stays,
        }
    }

    /// `fetched` installed from its copy: confirmed first, naming what it executes,
    /// where that waits on a person; asked at once otherwise.
    fn install(&mut self, fetched: &PackFetched) -> Next {
        let pack = fetched.pack.clone();
        if fetched.waits_on_yes {
            let executors = &fetched.audit.manifest.declares.executors;
            let asks = yunta_render::screen::executing(&pack.to_string(), executors);
            let held = fetched.held;
            let install = PackOperation::Install {
                held,
                confirmed: true,
            };
            let same = vec![fetched.installs_as.clone()];
            return Next::Confirm(Confirmation::new(asks, same, Operation::Pack(install)));
        }
        self.shown = PacksShown::Installing { pack };
        let held = fetched.held;
        let install = PackOperation::Install {
            held,
            confirmed: false,
        };
        Next::Work(Operation::Pack(install))
    }

    /// A pack fetched, where one was being fetched: its inventory read, or, for an
    /// update that ships no executor, installed at once, as `yunta pack update`
    /// installs it.
    pub(super) fn fetched(&mut self, fetched: Arc<PackFetched>) -> Next {
        if !matches!(self.shown, PacksShown::Fetching { .. }) {
            return Next::Release(fetched.held);
        }
        let ships_none = fetched.audit.manifest.declares.executors.is_empty();
        if fetched.updating_to.is_some() && ships_none {
            return self.install(&fetched);
        }
        self.shown = PacksShown::Fetched(fetched);
        Next::Stays
    }

    /// Whether an install's answer is waited on: where it was asked, confirmed or
    /// not.
    pub(super) fn awaits_the_install(&self) -> bool {
        matches!(
            self.shown,
            PacksShown::Installing { .. } | PacksShown::Fetched(_)
        )
    }
}

/// The screen while a pack is asked for, fetched or installed: the question, what is
/// under way, the inventory a person confirms against, or what installing came to;
/// with the keys.
pub(super) fn installing_screen(shown: &PacksShown, look: &Look) -> (Doc, Legend) {
    let page = yunta_render::screen::pack_page_legend();
    match shown {
        PacksShown::Asking(asked) => (
            Doc::new().with(asked.line().block(look)),
            yunta_render::screen::pack_naming_legend(),
        ),
        PacksShown::Fetching { what, .. } => {
            let fetching = Note(yunta_render::screen::fetching(what));
            (Doc::new().with(fetching), Legend::new())
        }
        PacksShown::Fetched(fetched) => (
            inventory(fetched).document(look),
            yunta_render::screen::fetched_legend(),
        ),
        PacksShown::Installing { pack } => {
            let installing = Note(yunta_render::screen::installing_pack(&pack.to_string()));
            (Doc::new().with(installing), Legend::new())
        }
        PacksShown::Installed(installed) => (installed_page(installed, look), page),
        PacksShown::Updated { pack, to, commit } => {
            let updated = Updated { pack, to, commit };
            (updated.document(look), page)
        }
        PacksShown::Listing
        | PacksShown::Auditing { .. }
        | PacksShown::Audited { .. }
        | PacksShown::Naming(_)
        | PacksShown::Scaffolding { .. }
        | PacksShown::Scaffolded(_) => (Doc::new(), Legend::new()),
    }
}

/// What a person reads of a pack fetched before it is installed, as `yunta pack add`
/// and `yunta pack update` show it.
fn inventory(fetched: &PackFetched) -> Inventory<'_> {
    let (audit, waits_on_yes) = (&fetched.audit, fetched.waits_on_yes);
    match fetched.updating_to {
        Some(_) => Inventory::Updating {
            audit,
            waits_on_yes,
        },
        None => Inventory::Installing {
            audit,
            waits_on_yes,
        },
    }
}

/// The page `yunta pack add` ends with, and what stopped reading what the pack's
/// workflows need where something did.
fn installed_page(installed: &PackInstalled, look: &Look) -> Doc {
    let unread = Needs::default();
    let needs = installed.needs.as_ref().unwrap_or(&unread);
    let page = Installed {
        manifest: &installed.manifest,
        commit: &installed.commit,
        at: &installed.at,
        cases: &installed.cases,
        needs,
    };
    let doc = page.document(look);
    match &installed.needs {
        Ok(_) => doc,
        Err(unread) => doc.with(Note(unread.clone())),
    }
}
