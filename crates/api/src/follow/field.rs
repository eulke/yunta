//! See [`super`]. The field's side of a look: the runs as far as a reach goes, each at
//! its head, followed through the following's one reader beside the run it reads whole.
//!
//! The field is read whole on the first look and on a refollow to another reach: the
//! runs' heads, then each run as a listing reads it too, its directory, the manifest it
//! froze and, when that manifest places it here, its log. Which repository is here is
//! asked of git once for each reach; a run placed in another repository is counted there
//! and read no further.
//! Past a commit a look reads the heads once and the events past the head of each run
//! that moved, folds them into the run and hands them as its news; a run new to the log
//! is read whole and joins, or is counted elsewhere; a run gone from the heads leaves.
//! Once a second it reads the registry of each run whose log says it moves or asks, and
//! hands a change after the look's events; every ten seconds it checks that each run it
//! reads still has its directory.
//!
//! The run read whole and its place in the field never disagree: to the following they
//! are one run. Its events are read once, by the field, and handed once, folded into
//! both copies; and whichever side reads the run first hands the other its copy, the run
//! read whole taking the field's with its log cut at the field's head. What lies past
//! that head comes with the next look past a commit, as everything committed after a read
//! does.
//!
//! Errors stay with their run. A run whose log or manifest fails a read is unreadable,
//! with why, and read again whole when its head moves; a run whose directory went is
//! unreadable for that, and stays so while its directory is gone, until it leaves the
//! log. The rest of the field goes on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::time::Instant;
use yunta_core::events::StoredEvent;
use yunta_core::{RunId, Seq};
use yunta_render::page::{RunUnread, UnreadableRun};
use yunta_replay::registry::Engine;

use super::following::{moves_or_asks, REGISTRY_EVERY};
use super::{FollowNews, FollowedRun, Following, RunNews, RunRegistry};
use crate::field::{Here, Placed};
use crate::{Error, Failure, FieldReach, Stop};

/// How often a following checks that each run of its field still has its directory. A
/// directory goes only when collection takes a run that ended long before, and its rows
/// go on a later pass: ten seconds says so while the rows still stand, at one look at
/// the disk a run and no read of the log.
pub(super) const DIRS_EVERY: Duration = Duration::from_secs(10);

/// The field as a following reads it whole: each run of it at its head, those it cannot
/// read with why, how many runs it leaves to other repositories, and whether it is this
/// repository's. Not [`FieldReading`](crate::FieldReading), the field read once for a
/// listing, a row a run.
#[derive(Debug, Clone)]
pub struct FollowedField {
    /// Each run of the field that reads, at its head.
    pub runs: Vec<FollowedRun>,
    /// The runs of the field it can name but not read.
    pub unreadable: Vec<UnreadableRun>,
    /// How many runs the log holds that it leaves to other repositories.
    pub elsewhere: usize,
    /// Whether it is the runs of the repository it was asked in, rather than every run
    /// on this machine.
    pub here: bool,
}

/// The field a following holds, as far as it read each run of it. Not
/// [`FollowedField`], what is handed of it.
pub(super) struct HeldField {
    /// How far it reaches.
    pub(super) reach: FieldReach,
    /// The repository here, asked of git once for this reach; none where the field
    /// reaches every run on this machine.
    here: Option<Here>,
    /// Each run of the field the log holds.
    runs: BTreeMap<RunId, InField>,
    /// The runs it leaves to other repositories, read no further.
    elsewhere: BTreeSet<RunId>,
    /// How many runs it left elsewhere when that was last handed.
    elsewhere_handed: usize,
    /// When the registries were last read: again once a second has passed.
    registries_read_at: Instant,
    /// When the directories were last checked: again once [`DIRS_EVERY`] has passed.
    pub(super) dirs_checked_at: Instant,
    /// The engines a look read that changed, handed after its events.
    engines_read: Vec<(RunId, Engine)>,
}

/// One run of the field, as far as it was read.
enum InField {
    /// Read: at its head, with the registry its engine keeps beside it.
    Read {
        run: Box<FollowedRun>,
        registry: RunRegistry,
    },
    /// Not read, for `why`, at the head its log had when it was last tried.
    Unreadable {
        why: UnreadableRun,
        head: Option<Seq>,
    },
}

impl HeldField {
    /// The field at `reach`, in the repository `here`, holding no run yet.
    fn new(reach: FieldReach, here: Option<Here>) -> HeldField {
        HeldField {
            reach,
            here,
            runs: BTreeMap::new(),
            elsewhere: BTreeSet::new(),
            elsewhere_handed: 0,
            registries_read_at: Instant::now(),
            dirs_checked_at: Instant::now(),
            engines_read: Vec::new(),
        }
    }

    /// `news` of `run` folded into the run where the field reads it: what came to its
    /// log and to its engine. The rest is news of the run read whole alone.
    pub(super) fn folded(&mut self, run: &RunId, news: &RunNews) {
        let Some(InField::Read { run: read, .. }) = self.runs.get_mut(run) else {
            return;
        };
        if matches!(news, RunNews::Events(_) | RunNews::Engine(_)) {
            read.folded(news);
        }
    }
}

impl Following {
    /// `news` of `run` folded into each copy the following keeps of it, the run held and
    /// its place in the field, and kept to hand back once for both.
    pub(super) fn found_of(&mut self, run: RunId, news: RunNews) {
        if let Some(held) = self.held.as_mut().filter(|held| held.run.run == run) {
            held.run.folded(&news);
        }
        if let Some(field) = self.field.as_mut() {
            field.folded(&run, &news);
        }
        self.news.push_back(FollowNews::Run { run, news });
    }

    /// Whether the field holds the run `run`, read or not: its log is then the field's
    /// to read.
    pub(super) fn field_holds(&self, run: &RunId) -> bool {
        self.field
            .as_ref()
            .is_some_and(|field| field.runs.contains_key(run))
    }

    /// The run `run` as the field reads it, its registry, and its log up to the head the
    /// field holds it at: what the run read whole takes from the field, so the two stand
    /// at one head. None where the field does not read it.
    pub(super) async fn whole_from_the_field(
        &self,
        run: &RunId,
    ) -> Result<Option<(FollowedRun, RunRegistry, Vec<StoredEvent>)>, Error> {
        let held = self.field.as_ref().and_then(|field| field.runs.get(run));
        let Some(InField::Read {
            run: read,
            registry,
        }) = held
        else {
            return Ok(None);
        };
        let head = read.state.head();
        let events = self.reader.events(run).await.map_err(Failure::new)?;
        let events = events
            .into_iter()
            .take_while(|event| Some(event.seq) <= head)
            .collect();
        Ok(Some(((**read).clone(), registry.clone(), events)))
    }

    /// The field followed read whole and handed, when it is not the one held: the runs'
    /// heads, each run of them placed and read, and which repository is here, asked of
    /// git under `stop`.
    pub(super) async fn read_field_whole(&mut self, stop: &Stop) -> Result<(), Error> {
        let held = self.field.as_ref().map(|field| field.reach);
        let Some(reach) = self.followed.field.filter(|reach| held != Some(*reach)) else {
            return Ok(());
        };
        let here = match reach {
            FieldReach::Machine => None,
            FieldReach::Here => Here::of(&self.project, stop).await?,
        };
        let heads = self.reader.heads().await.map_err(Failure::new)?;
        let mut field = HeldField::new(reach, here);
        let (mut runs, mut unreadable) = (Vec::new(), Vec::new());
        for head in heads {
            let run = head.run_id;
            match self.placed(field.here.as_ref(), &run).await {
                Placed::Read(read, registry) => {
                    runs.push((*read).clone());
                    let read = InField::Read {
                        run: read,
                        registry,
                    };
                    field.runs.insert(run, read);
                }
                Placed::Unreadable(why) => {
                    unreadable.push(why.clone());
                    let head = Some(head.seq);
                    field.runs.insert(run, InField::Unreadable { why, head });
                }
                Placed::Elsewhere => {
                    field.elsewhere.insert(run);
                }
            }
        }
        field.elsewhere_handed = field.elsewhere.len();
        let whole = FollowedField {
            runs,
            unreadable,
            elsewhere: field.elsewhere_handed,
            here: field.here.is_some(),
        };
        self.field = Some(field);
        self.news.push_back(FollowNews::Field(Box::new(whole)));
        Ok(())
    }

    /// What came to the field since a commit: the runs' heads read once; the events
    /// past the head of each run read that moved, handed as its news; a run new to the
    /// log, or one unread whose head moved, read whole; a run gone from the heads left;
    /// and how many runs the field leaves elsewhere, handed once that changed.
    pub(super) async fn read_field_past_the_heads(&mut self) -> Result<(), Error> {
        if self.field.is_none() {
            return Ok(());
        }
        let heads = self.reader.heads().await.map_err(Failure::new)?;
        let logged: BTreeSet<RunId> = heads.iter().map(|head| head.run_id.clone()).collect();
        for head in heads {
            let Some(field) = &self.field else {
                break;
            };
            if field.elsewhere.contains(&head.run_id) {
                continue;
            }
            match field.runs.get(&head.run_id) {
                Some(InField::Read { run, .. }) if run.state.head() < Some(head.seq) => {
                    self.read_on(head.run_id, head.seq).await;
                }
                Some(InField::Unreadable { head: tried, .. }) if *tried != Some(head.seq) => {
                    self.read_again(head.run_id, head.seq).await;
                }
                Some(_) => {}
                None => self.read_again(head.run_id, head.seq).await,
            }
        }
        self.left_the_log(&logged);
        Ok(())
    }

    /// The registry of each run of the field whose log says it moves or asks a person,
    /// read once a second has passed since the last reading, a change kept to hand after
    /// the look's events. `now` frames the runs.
    pub(super) fn read_field_registries(&mut self, now: DateTime<Utc>) {
        let Some(field) = self.field.as_mut() else {
            return;
        };
        if field.registries_read_at.elapsed() < REGISTRY_EVERY {
            return;
        }
        field.registries_read_at = Instant::now();
        for (id, run) in &field.runs {
            let InField::Read { run, registry } = run else {
                continue;
            };
            if !moves_or_asks(&run.frame(now)) {
                continue;
            }
            let engine = registry.engine();
            if engine != run.engine {
                field.engines_read.push((id.clone(), engine));
            }
        }
    }

    /// Each engine a registry of the field named changed, handed after the look's
    /// events as its run's news, where the field still reads the run.
    pub(super) fn hand_field_engines(&mut self) {
        let Some(field) = self.field.as_mut() else {
            return;
        };
        let read = std::mem::take(&mut field.engines_read);
        for (run, engine) in read {
            let reads = self
                .field
                .as_ref()
                .is_some_and(|field| matches!(field.runs.get(&run), Some(InField::Read { .. })));
            if reads {
                self.found_of(run, RunNews::Engine(engine));
            }
        }
    }

    /// That each run the field reads still has its directory, once [`DIRS_EVERY`] has
    /// passed since the last check: a run whose directory went is said unreadable.
    pub(super) fn check_field_dirs(&mut self) {
        let Some(field) = self.field.as_mut() else {
            return;
        };
        if field.dirs_checked_at.elapsed() < DIRS_EVERY {
            return;
        }
        field.dirs_checked_at = Instant::now();
        let gone: Vec<(RunId, Option<Seq>)> = field
            .runs
            .iter()
            .filter_map(|(id, run)| match run {
                InField::Read { run, registry } if !has_its_directory(registry.run_dir()) => {
                    Some((id.clone(), run.state.head()))
                }
                InField::Read { .. } | InField::Unreadable { .. } => None,
            })
            .collect();
        for (run, head) in gone {
            let runs_root = self.project.runs_root.clone();
            self.unreadable(run, RunUnread::NoDirectory { runs_root }, head);
        }
    }

    /// `run` read for the field and placed by `here`, as every field reads a run, the
    /// run held's own copy standing for it when it is the run read whole.
    async fn placed(&self, here: Option<&Here>, run: &RunId) -> Placed {
        let held = self.held.as_ref().filter(|held| held.run.run == *run);
        let held = held.map(|held| (&held.run, &held.registry));
        Placed::of(&self.project, &self.reader, here, run, held).await
    }

    /// The events of the run `run` past the head the field holds it at, folded in and
    /// handed as its news; a log that fails the read makes the run unreadable at `head`,
    /// the head of its log now.
    async fn read_on(&mut self, run: RunId, head: Seq) {
        let held = self.field.as_ref().and_then(|field| field.runs.get(&run));
        let Some(InField::Read { run: read, .. }) = held else {
            return;
        };
        let past = match read.state.head() {
            Some(from) => self.reader.events_after(&run, from).await,
            None => self.reader.events(&run).await,
        };
        match past {
            Ok(past) if past.is_empty() => {}
            Ok(past) => self.found_of(run, RunNews::Events(past)),
            Err(unread) => {
                let problem = RunUnread::EventLog(unread.to_string());
                self.unreadable(run, problem, Some(head));
            }
        }
    }

    /// The run `run`, new to the field or unread when last tried, read whole now that its
    /// log's head is `head`: it joins the field and is read on to that head, is said
    /// unreadable when why changed, or is counted elsewhere, leaving the field if it
    /// stood in it.
    async fn read_again(&mut self, run: RunId, head: Seq) {
        let Some(field) = &self.field else {
            return;
        };
        let placed = self.placed(field.here.as_ref(), &run).await;
        let Some(field) = self.field.as_mut() else {
            return;
        };
        let was = field.runs.remove(&run);
        match placed {
            Placed::Read(read, registry) => {
                let behind = read.state.head() < Some(head);
                self.news.push_back(FollowNews::Joined(read.clone()));
                let read = InField::Read {
                    run: read,
                    registry,
                };
                field.runs.insert(run.clone(), read);
                if behind {
                    self.read_on(run, head).await;
                }
            }
            Placed::Unreadable(why) => {
                let said = matches!(&was, Some(InField::Unreadable { why: was, .. }) if was.problem == why.problem);
                if !said {
                    self.news.push_back(FollowNews::Unreadable(why.clone()));
                }
                let head = Some(head);
                field.runs.insert(run, InField::Unreadable { why, head });
            }
            Placed::Elsewhere => {
                field.elsewhere.insert(run.clone());
                if was.is_some() {
                    self.news.push_back(FollowNews::Left(run));
                }
            }
        }
    }

    /// The run `run` held unreadable for `problem` at `head`, and said so.
    fn unreadable(&mut self, run: RunId, problem: RunUnread, head: Option<Seq>) {
        let Some(field) = self.field.as_mut() else {
            return;
        };
        let why = UnreadableRun::new(&run, problem);
        self.news.push_back(FollowNews::Unreadable(why.clone()));
        field.runs.insert(run, InField::Unreadable { why, head });
    }

    /// The runs of the field gone from `logged`, the runs the log's heads name, left;
    /// and how many runs the field leaves elsewhere, handed once that changed.
    fn left_the_log(&mut self, logged: &BTreeSet<RunId>) {
        let Some(field) = self.field.as_mut() else {
            return;
        };
        let gone: Vec<RunId> = field
            .runs
            .keys()
            .filter(|run| !logged.contains(*run))
            .cloned()
            .collect();
        for run in gone {
            field.runs.remove(&run);
            self.news.push_back(FollowNews::Left(run));
        }
        field.elsewhere.retain(|run| logged.contains(run));
        if field.elsewhere.len() != field.elsewhere_handed {
            field.elsewhere_handed = field.elsewhere.len();
            self.news
                .push_back(FollowNews::Elsewhere(field.elsewhere_handed));
        }
    }
}

/// Whether the run whose directory is `dir` still has it, as a run is found by: the
/// manifest it froze stands there.
fn has_its_directory(dir: &Path) -> bool {
    yunta_engine::run_dir::manifest_path(dir).exists()
}
