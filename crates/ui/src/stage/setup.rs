//! See [`super`]. A repository Yunta was never set up in, set up from one screen of rows:
//! what the repository is, a fact a row, the project's name and its base branch among them;
//! who does the work, a role a row with its candidates in the order they are written; and
//! the row that writes it all. As it opens it asks what the repository runs and the adapters
//! this machine answers for, and the pack a setup installs fetched, work of its own beside
//! the detection, so a person waits for the slower of the two: the rows stand when the
//! detection lands, and the `workflow` row and the roles once the fetch has landed too,
//! whichever came first.
//!
//! The pointer starts on the row that writes; `j k` (and the arrows) step along the rows that
//! change: the name, the base, each role, the row that writes. `enter` on a row changes it in
//! place: the name or the base in a line typed under it, an empty one keeping what stands; a
//! role in a row of the adapters this machine answers for, `←→` choosing and `enter` taking
//! one, then its model in a line typed under it, an empty one leaving it to the adapter. The
//! role's other candidates follow the one taken in the order the machine found them. `esc`
//! gives the row back as it stood. `enter` on the row that writes writes it: the answers
//! become the setup `init -i` writes from the same ones ([`ProjectDraft::answered`]),
//! installing the pack fetched with the project as its row says. `q` leaves with nothing
//! written.
//!
//! Where the write installs the pack, `enter` on that row first shows the pack's audit, as
//! `yunta pack add` shows it before it installs, a page the arrows scroll, and `esc` goes back
//! to the rows as they stood; `enter` there asks its install confirmed, saying what writing
//! touches and the commands that do the same at a shell. Confirmed, the pack is installed and
//! the setup written; `esc` on the confirmation writes the setup without the pack, as `init`
//! writes it with the same answers, none of the pack's roles proposed. Where the pack was not
//! fetched, out of reach or installed already, the row writes the setup alone at once.
//!
//! What was written stands, the page `yunta pack add` ends with where the pack was installed,
//! what it still needs from the project among it, and `init`'s page otherwise; a note says
//! what writing touched, and `enter` goes on to the first request; `n`, the repository now
//! set up, opens a new run over it.
//!
//! The performer holds the project detected until its setup is written, under the ticket
//! that detected it, and the pack beside it once fetched, under the fetch's own ticket until
//! the project lands: the stage says which ([`SetupStage::held`]), so leaving it lets both
//! go. What refused or failed to write is said, and the repository found and its pack
//! fetched again, to be asked for again.
//!
//! The state follows the field of the repository's runs while a setup stands, as it does
//! under the first request: the tally and the attention row are the one composer's
//! (`FollowedRuns::framing`), counting this repository's runs. Where no line is typed, `a`
//! goes to the run the attention row names, as everywhere a person does not type, its screen
//! opened over the setup, which stands at the bottom and is never dropped, and the row says
//! so; where one is, the row names the run with no key. Going down from that run, as `q`
//! does, comes back to the setup. No run is asked for here: the repository is not set up. The
//! page's sentence opens on what that field holds (`RepositoryPast`): a repository whose
//! field holds a run of it has used yunta, and only this checkout holds no setup. Until the
//! field is first read the page says no sentence, so none is said and then taken back.

mod keys;
mod pages;
mod rows;
mod writing;

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use surco::components::{Horizon, Laid, Legend, Measure, Note, Region, Screen, Wordmark};
use surco::doc::{Block, Doc};
use surco::line::Tone;
use surco::look::Look;
use yunta_api::{Committer, FieldReach, InitializedProject, ProjectDraft, ProjectSetup};
use yunta_render::page::{Candidates, RepositoryPast};

use super::over_the_field::attending_from_a_setup;
use super::Want;
use crate::followed_runs::Standpoint;
use crate::prompt::Stroke;
use crate::scroll::Scroll;
use crate::{
    FollowedRuns, InstallsThePack, Operation, Outcome, PackInstalled, SetupPackFetch, Ticket,
};
use pages::{audited, found, written};
use rows::{Changing, Pressed, Rows};

/// The cells the setup's rows stand in, centred on a wider screen: a role's first candidate
/// in its own column of 32 and the others after it, beside the widest label, as the design
/// sets them.
const MEASURE: usize = 92;

/// The cells inside a screen's margins from which a role's candidates after its first stand
/// on its row: a screen 100 cells wide inside its margins, 102 columns, as every region beside
/// another stands from.
const EVERY_CANDIDATE_FROM: usize = 100;

/// A repository being set up.
#[derive(Debug, Clone)]
pub struct SetupStage {
    /// Where setting it up stands.
    setting: Setting,
    /// What refused or failed, where something did.
    said: Option<String>,
}

/// Where setting a repository up stands.
#[derive(Debug, Clone)]
enum Setting {
    /// What it runs being found, and the pack a setup installs beside it, where its fetch
    /// landed first: what fetching it came to, and the ticket that holds its copy until the
    /// project it is held with is detected.
    Finding {
        fetched: Option<(Ticket, Arc<SetupPackFetch>)>,
    },
    /// Nothing found: what refused or failed stands, and leaving is left.
    Unfound,
    /// What was found, its rows read and changed in place.
    Rows(Box<Rows>),
    /// The audit of the pack the write installs, read before its install is confirmed, where
    /// a person scrolled it; the rows kept as they stood, which `esc` goes back to.
    Audit { rows: Box<Rows>, scroll: Scroll },
    /// The setup answered, being written.
    Writing {
        found: Box<Found>,
        setup: Box<ProjectSetup>,
    },
    /// What was written into `repo`, what was left as it was, and the pack installed with it,
    /// where one was.
    Written {
        repo: PathBuf,
        initialized: Box<InitializedProject>,
        installed: Option<Arc<PackInstalled>>,
    },
}

/// What setting the repository up found: the project as detected, who git commits a run's
/// work as there, the ticket the performer holds them under, and the pack the setup installs
/// once fetching it lands.
#[derive(Debug, Clone)]
struct Found {
    draft: ProjectDraft,
    /// The ticket the performer holds the project detected under.
    held: Ticket,
    committer: Committer,
    /// What fetching the pack came to; none while it is fetched.
    pack: Option<Arc<SetupPackFetch>>,
}

impl SetupStage {
    /// A repository's setup, with what it runs yet to be found.
    pub(crate) fn new() -> Self {
        SetupStage {
            setting: Setting::Finding { fetched: None },
            said: None,
        }
    }

    /// What the stage asks for as it opens: what setting the repository up finds, and the
    /// pack a setup installs fetched beside it, so a person waits for the slower of the two.
    pub(crate) fn opening(&self) -> Vec<Want> {
        vec![
            Want::Work(Operation::DetectProject),
            Want::Work(Operation::FetchSetupPack),
        ]
    }

    /// The work its screen says is under way: what the repository runs found, or the setup
    /// written, as it was asked, until it ends.
    pub(crate) fn shown_under_way(&self) -> Option<Operation> {
        match &self.setting {
            Setting::Finding { .. } => Some(Operation::DetectProject),
            Setting::Writing { found, setup } => Some(Operation::WriteSetup {
                held: found.held,
                setup: (**setup).clone(),
                pack: found.installs(),
            }),
            Setting::Unfound
            | Setting::Rows(_)
            | Setting::Audit { .. }
            | Setting::Written { .. } => None,
        }
    }

    /// The ticket the performer holds what the setup found under: the project detected, with
    /// the pack once it landed, while the setup is read and changed; the pack alone, where it
    /// was fetched before the project was detected. What leaving the stage lets go.
    pub(crate) fn held(&self) -> Option<Ticket> {
        match &self.setting {
            Setting::Rows(rows) | Setting::Audit { rows, .. } => Some(rows.found.held),
            Setting::Finding {
                fetched: Some((held, fetched)),
            } => writing::held_copy(*held, fetched),
            Setting::Finding { fetched: None }
            | Setting::Unfound
            | Setting::Writing { .. }
            | Setting::Written { .. } => None,
        }
    }

    /// The setup as a screen of what `followed` keeps at `now`, a line typed in what stands
    /// over it where `typed`: its region; where it stands on the trail, the pack audited
    /// after it; the tally and the attention row, counting the repository's runs, the row
    /// naming its key where no line is typed; what writing touched, once it is written; and
    /// its keys.
    pub(crate) fn screen(
        &self,
        look: &Look,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
        typed: bool,
    ) -> Screen {
        let mut screen = Screen::new(self.region(look, past(followed)));
        screen.trail = vec![
            yunta_render::screen::YUNTA.to_string(),
            yunta_render::screen::SETUP.to_string(),
        ];
        if let Setting::Audit { rows, .. } = &self.setting {
            let pack = rows.found.pack.as_deref().map(SetupPackFetch::pack);
            screen.trail.extend(pack.map(ToString::to_string));
        }
        let at = Standpoint {
            run: None,
            typed: self.typed(),
        };
        (screen.tally, screen.attention) = followed.framing(at, now);
        screen.noted = self.noted();
        if let Some(legend) = self.legend() {
            screen.legend = legend;
        }
        screen.typed = self.typed();
        screen.anywhere = match typed || self.typed() {
            true => Legend::new(),
            false => yunta_render::screen::anywhere(),
        };
        screen
    }

    /// The region of the screen at `look`'s width, the field of the repository's runs holding
    /// `past`: what is being found; the setup's rows, the row being changed opened in place,
    /// set to the setup's measure over the field in perspective; the pack's audit, where it
    /// was scrolled to; the setup being written; or what was written. What refused or failed
    /// is said under it.
    fn region(&self, look: &Look, past: Option<RepositoryPast>) -> Region {
        let candidates = match look.width.cells() >= EVERY_CANDIDATE_FROM {
            true => Candidates::Every,
            false => Candidates::First,
        };
        let said = self.said.iter().map(|said| Note(said.clone()));
        let (survey, said) = match &self.setting {
            Setting::Rows(rows) => {
                let opened = (Some(&rows.pointed), rows.changing.as_ref());
                let survey = found(&rows.found, &rows.setup(), past, opened, look, candidates);
                (survey, said.collect())
            }
            Setting::Writing { found: what, setup } => {
                let survey = found(what, setup, past, (None, None), look, candidates);
                let writing = yunta_render::screen::WRITING_THE_SETUP.to_string();
                (survey, vec![Note(writing)])
            }
            Setting::Finding { .. } => {
                let finding = Note(yunta_render::screen::FINDING_WHAT_IT_RUNS.to_string());
                return Region::new(Doc::new().with(finding).with_all(said.map(Into::into)));
            }
            Setting::Unfound => return Region::new(Doc::new().with_all(said.map(Into::into))),
            Setting::Audit { rows, scroll } => {
                let page = audited(&rows.found, look).with_all(said.map(Into::into));
                return Region::of_a_page(page).in_sight(scroll.sight());
            }
            Setting::Written {
                repo,
                initialized,
                installed,
            } => {
                let page = written(repo, initialized, installed.as_deref(), look);
                return Region::of_a_page(page.with_all(said.map(Into::into)));
            }
        };
        let doc = Doc::new()
            .with(Wordmark)
            .with(Block::Survey(survey))
            .with_all(said.into_iter().map(Into::into))
            .with(Horizon { tone: Tone::Faint });
        Region::new(doc).measured(Measure::of(MEASURE))
    }

    /// The keys the screen lists: leaving while nothing is found; on the rows, those of the
    /// row pointed at, of the adapter being chosen or of the line being typed; on the pack's
    /// audit, scrolling it, asking its install and going back; going on once the setup is
    /// written. None while it is being written.
    fn legend(&self) -> Option<Legend> {
        match &self.setting {
            Setting::Finding { .. } | Setting::Unfound => {
                Some(yunta_render::screen::leaving_legend())
            }
            Setting::Rows(rows) => Some(rows.legend()),
            Setting::Audit { .. } => Some(yunta_render::screen::setup_audit_legend()),
            Setting::Writing { .. } => None,
            Setting::Written { .. } => Some(yunta_render::screen::written_legend()),
        }
    }

    /// What `stroke` asks for here, over what `followed` keeps at `now`: what the rows make
    /// of it, the setup written from the row that writes, or the pack's audit shown first
    /// where the write installs it; on the audit, the keys that scroll scroll it, Enter asks
    /// its install confirmed and Escape goes back to the rows; Enter on what was written goes
    /// on to the first run, and `n` opens a new run there, the repository set up; `q` or
    /// Ctrl-C leaves where no line is typed on, nothing written.
    pub(crate) fn pressed(
        &mut self,
        stroke: Stroke,
        followed: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Want> {
        match &mut self.setting {
            Setting::Rows(rows) => match rows.pressed(stroke) {
                Pressed::Stays => {
                    self.said = None;
                    Vec::new()
                }
                Pressed::Refused(said) => {
                    self.said = Some(said);
                    Vec::new()
                }
                Pressed::Write => self.on_the_write_row(),
                Pressed::Attend => attending_from_a_setup(followed, now),
                Pressed::Leave => vec![Want::Leave],
            },
            Setting::Audit { scroll, .. } => match stroke {
                stroke if Scroll::moves_by(stroke) => {
                    scroll.pressed(stroke);
                    Vec::new()
                }
                Stroke::Enter => self.confirming(),
                Stroke::Decline => {
                    self.back_to_the_rows();
                    Vec::new()
                }
                Stroke::Insert('a') => attending_from_a_setup(followed, now),
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
            Setting::Written { .. } => keys::pressed_on_what_was_written(stroke, followed),
            Setting::Finding { .. } | Setting::Unfound => match stroke {
                Stroke::Insert('q') | Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
            Setting::Writing { .. } => match stroke {
                Stroke::Interrupt => vec![Want::Leave],
                _ => Vec::new(),
            },
        }
    }

    /// Whether the screen is one a line is typed on: a row is being changed in place, so
    /// every printable key goes to it and none to the screen.
    pub(crate) fn typed(&self) -> bool {
        match &self.setting {
            Setting::Rows(rows) => rows.changing.is_some(),
            Setting::Finding { .. }
            | Setting::Unfound
            | Setting::Audit { .. }
            | Setting::Writing { .. }
            | Setting::Written { .. } => false,
        }
    }

    /// What a surface laid out of the screen's region, which the keys that scroll the pack's
    /// audit move from.
    pub(crate) fn laid(&mut self, laid: Laid) {
        if let Setting::Audit { scroll, .. } = &mut self.setting {
            scroll.laid(laid);
        }
    }

    /// Whether words are being typed: the name, the base or a role's model, in a line typed
    /// in place. Not a role's adapter being chosen, which types nothing.
    pub(crate) fn typing(&self) -> bool {
        let Setting::Rows(rows) = &self.setting else {
            return false;
        };
        matches!(
            rows.changing,
            Some(
                Changing::Typed(_)
                    | Changing::Role {
                        on_the_model: true,
                        ..
                    }
            )
        )
    }

    /// What an answer to the setup's work does, wherever the setup stands: what was found
    /// stands as rows, with the pack a setup installs where its fetch landed first; the pack,
    /// landing after, fills its row and the roles; what was written stands; what refused or
    /// failed finding the repository or writing it is said.
    pub(crate) fn answered(&mut self, outcome: Outcome) -> Vec<Want> {
        match outcome {
            Outcome::Detected {
                draft,
                held,
                committer,
            } => {
                self.detected(*draft, held, committer);
                Vec::new()
            }
            Outcome::SetupPack { fetched, held } => self.pack_landed(fetched, held),
            Outcome::Initialized {
                initialized,
                installed,
            } => {
                if let Setting::Writing { found, .. } = &self.setting {
                    let repo = found.draft.repo.clone();
                    self.setting = Setting::Written {
                        repo,
                        initialized,
                        installed,
                    };
                }
                Vec::new()
            }
            Outcome::Refused(refused) => self.unwritten(refused.to_string()),
            Outcome::Failed(said) => self.unwritten(said),
            Outcome::Repository(_)
            | Outcome::Catalog(_)
            | Outcome::Workflow(_)
            | Outcome::Pack(_)
            | Outcome::Evidence(_)
            | Outcome::PullRequest(_)
            | Outcome::History(_)
            | Outcome::Graph(_)
            | Outcome::Doctor(_)
            | Outcome::Collected(_)
            | Outcome::Decided { .. }
            | Outcome::Answered { .. }
            | Outcome::Started { .. }
            | Outcome::Resumed { .. }
            | Outcome::Cancelled { .. }
            | Outcome::Closed { .. }
            | Outcome::Stopped => Vec::new(),
        }
    }

    /// What was found, `draft` and `committer`, held under `held`, standing as rows, with the
    /// pack a setup installs where its fetch landed first.
    fn detected(&mut self, draft: ProjectDraft, held: Ticket, committer: Committer) {
        let pack = match &mut self.setting {
            Setting::Finding { fetched } => fetched.take().map(|(_, fetched)| fetched),
            _ => None,
        };
        let found = Found {
            draft,
            held,
            committer,
            pack,
        };
        self.setting = Setting::Rows(Box::new(Rows::new(found)));
    }

    /// What fetching the pack a setup installs came to, `fetched` under `held`: filling its row
    /// and the roles on the rows, or kept until the detection lands where it lands first; its
    /// copy let go where nothing was found to write it with.
    fn pack_landed(&mut self, fetched: Arc<SetupPackFetch>, held: Ticket) -> Vec<Want> {
        match &mut self.setting {
            Setting::Rows(rows) => rows.fetched(fetched),
            Setting::Finding { fetched: landed } => *landed = Some((held, fetched)),
            Setting::Unfound => {
                let copy = writing::held_copy(held, &fetched);
                return copy.map(Want::Release).into_iter().collect();
            }
            Setting::Audit { .. } | Setting::Writing { .. } | Setting::Written { .. } => {}
        }
        Vec::new()
    }
}

impl Found {
    /// The project as the setup proposes it: proposing the roles of the pack fetched with it,
    /// which its write installs, where one was; as detected otherwise.
    fn proposing(&self) -> ProjectDraft {
        let mut draft = self.draft.clone();
        if let Some(SetupPackFetch::Fetched { proposed, .. }) = self.pack.as_deref() {
            draft.proposed = proposed.clone();
        }
        draft
    }

    /// Whether the write installs the pack: where one was fetched with the project.
    fn installs(&self) -> InstallsThePack {
        match self.pack.as_deref() {
            Some(SetupPackFetch::Fetched { .. }) => InstallsThePack::Yes,
            Some(SetupPackFetch::Installed { .. } | SetupPackFetch::OutOfReach { .. }) | None => {
                InstallsThePack::No
            }
        }
    }
}

/// What the repository did with yunta before, as the field of its runs that `followed` keeps
/// holds them: none until that field is read here.
fn past(followed: &FollowedRuns) -> Option<RepositoryPast> {
    let field = followed.field.as_ref();
    let here = field.filter(|field| field.reach == FieldReach::Here)?;
    Some(match here.holds_a_run() {
        true => RepositoryPast::Used,
        false => RepositoryPast::NeverUsed,
    })
}
