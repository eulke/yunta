//! See [`super`]. A setup written: from the row that writes, at once or through the audit of
//! the pack the write installs and its confirmation, with the pack or without it; what was
//! written noted; and what refused or failed to write said, the repository found again.

use surco::components::Noted;
use surco::glyphs::Mark;
use yunta_core::step::NextStep;
use yunta_render::screen::Confirming;

use super::{Found, Setting, SetupStage};
use crate::dialog::Confirmation;
use crate::scroll::Scroll;
use crate::stage::Want;
use crate::{InstallsThePack, Operation, SetupPackFetch, Ticket};

impl SetupStage {
    /// What writing touched, once the setup is written: the configuration, the pack installed
    /// with it where one was, and what it touched outside `.yunta/`, as what it wrote names
    /// them. Nothing before.
    pub(super) fn noted(&self) -> Option<Noted> {
        let Setting::Written {
            initialized,
            installed,
            ..
        } = &self.setting
        else {
            return None;
        };
        let pack = installed
            .as_ref()
            .map(|installed| installed.manifest.reference().to_string());
        let said = yunta_render::screen::wrote_the_setup(pack.as_deref(), &initialized.wrote);
        Some(Noted {
            mark: Mark::Done,
            said,
        })
    }

    /// `enter` on the row that writes: where the write installs the pack fetched with the
    /// project, the pack's audit, read before its install is confirmed; the setup the rows
    /// make written at once otherwise, alone.
    pub(super) fn on_the_write_row(&mut self) -> Vec<Want> {
        match std::mem::replace(&mut self.setting, Setting::Unfound) {
            Setting::Rows(rows) if rows.found.installs() == InstallsThePack::Yes => {
                let scroll = Scroll::default();
                self.setting = Setting::Audit { rows, scroll };
                Vec::new()
            }
            Setting::Rows(rows) => {
                let write = Operation::WriteSetup {
                    held: rows.found.held,
                    setup: rows.setup(),
                    pack: InstallsThePack::No,
                };
                self.writing(rows.found, write)
            }
            other => {
                self.setting = other;
                Vec::new()
            }
        }
    }

    /// Escape on the pack's audit: the rows as they stood.
    pub(super) fn back_to_the_rows(&mut self) {
        if let Setting::Audit { rows, .. } = std::mem::replace(&mut self.setting, Setting::Unfound)
        {
            self.setting = Setting::Rows(rows);
        }
    }

    /// The confirmation the pack's audit asks before the pack is installed: what writing with
    /// it touches, the commands that do the same at a shell (`yunta init` where the rows make
    /// what it writes asking nothing, `yunta init -i` where a row was changed), the setup the
    /// rows make written with the pack installed first, and, going back from it, the setup
    /// written without it, none of the pack's roles proposed, as `init` writes it with the same
    /// answers.
    pub(super) fn confirming(&self) -> Vec<Want> {
        let Setting::Audit { rows, .. } = &self.setting else {
            return Vec::new();
        };
        let Some(SetupPackFetch::Fetched {
            pack, installs_as, ..
        }) = rows.found.pack.as_deref()
        else {
            return Vec::new();
        };
        let asks = Confirming::SetUpWith {
            pack: pack.to_string(),
            writes: rows.found.draft.writes(),
        };
        let init = NextStep::Init {
            interactive: !rows.as_proposed(),
        };
        let same = vec![installs_as.clone(), init];
        let held = rows.found.held;
        let with = Operation::WriteSetup {
            held,
            setup: rows.setup(),
            pack: InstallsThePack::Yes,
        };
        let without = Operation::WriteSetup {
            held,
            setup: rows.setup_without_the_pack(),
            pack: InstallsThePack::No,
        };
        let confirmation = Confirmation::new(asks, same, with).declined_as(without);
        vec![Want::Confirm(confirmation)]
    }

    /// What the pack's confirmation, answered, asks: `operation`, the setup written with the
    /// pack or without it, the rows standing while it is written as it writes them.
    pub(crate) fn confirmed(&mut self, operation: Operation) -> Vec<Want> {
        match std::mem::replace(&mut self.setting, Setting::Unfound) {
            Setting::Audit { rows, .. } => self.writing(rows.found, operation),
            other => {
                self.setting = other;
                vec![Want::Work(operation)]
            }
        }
    }

    /// `write`, the setup of what `found` found asked to be written with the project held under
    /// its ticket: the rows stand as it writes them while it is, the pack fetched with the
    /// project among them only where the write installs it.
    fn writing(&mut self, mut found: Found, write: Operation) -> Vec<Want> {
        let Operation::WriteSetup { setup, pack, .. } = &write else {
            return vec![Want::Work(write)];
        };
        let fetched = matches!(found.pack.as_deref(), Some(SetupPackFetch::Fetched { .. }));
        if fetched && *pack == InstallsThePack::No {
            found.pack = None;
        }
        self.setting = Setting::Writing {
            found: Box::new(found),
            setup: Box::new(setup.clone()),
        };
        self.said = None;
        vec![Want::Work(write)]
    }

    /// `said` said: of a setup being written, which the project was taken to write, the
    /// repository found again and its pack fetched again, to be asked for again; of what
    /// finding it came to, with nothing found, the pack fetched for it let go where it landed
    /// first, since nothing is written with it.
    pub(super) fn unwritten(&mut self, said: String) -> Vec<Want> {
        self.said = Some(said);
        match &self.setting {
            Setting::Writing { .. } => {
                self.setting = Setting::Finding { fetched: None };
                self.opening()
            }
            Setting::Finding { fetched } => {
                let held = fetched
                    .as_ref()
                    .and_then(|(held, fetched)| held_copy(*held, fetched));
                self.setting = Setting::Unfound;
                held.map(Want::Release).into_iter().collect()
            }
            Setting::Unfound
            | Setting::Rows(_)
            | Setting::Audit { .. }
            | Setting::Written { .. } => Vec::new(),
        }
    }
}

/// The ticket the performer holds the copy of the pack a setup installs under, `held`, where
/// fetching it came to one: none where it was out of reach or installed already.
pub(super) fn held_copy(held: Ticket, fetched: &SetupPackFetch) -> Option<Ticket> {
    matches!(fetched, SetupPackFetch::Fetched { .. }).then_some(held)
}
