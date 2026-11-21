//! See [`super`]. When the application calls the person, and what the window's title
//! says while a run needs them.
//!
//! A run the state keeps, of the field or in view, that turns to need the person calls
//! them once ([`Request::CallThePerson`]). The state keeps the runs that need them, each
//! as its row stood at the news that last moved it, and only news that can move a run's
//! standing (its events, its engine, a successor, the run read whole, joining or leaving
//! the field) frames that run again. A run read whole calls nobody: a field read whole
//! stands as it reads, so the runs that need the person as the application opens, or as
//! the field reaches further, call nothing, and before the field is first read nothing is
//! kept; a run read whole as it is opened stands as read too. A run that joins the field
//! already needing them, or a promotion's successor that does, calls them, since it is
//! new. News of the same pause leave a run needing them, and call nothing more.
//!
//! The window's title names the run in view where it needs the person; else the field's
//! run that has waited longest on them among those answered here, which a run asking at
//! its own terminal never is; none otherwise. Both are chosen as news folds, since a
//! run's standing moves only with its news and how long two runs have waited moves alike
//! with the clock, and the title says only which run and what it runs: a draw frames no
//! run. The title is the screen's, for a surface that has a window to name.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use surco::glyphs::Glyphs;
use surco::words::Standing;
use yunta_api::{FollowNews, FollowedRun, RunNews};
use yunta_core::RunId;
use yunta_render::page::FieldRow;
use yunta_render::watched;

use crate::followed_runs::waiting_longest;
use crate::{FollowedRuns, Request};

/// The runs the state keeps that stand needing the person, as the news folded last left
/// them: what tells a run that turns to need them from one that already did, and what the
/// window's title names. Not the field's group of runs that need a person, which a broken
/// run stands in too.
#[derive(Debug, Clone, Default)]
pub(super) struct Calling {
    /// The runs that need the person, each as its row stood at the news that last moved
    /// it; none before the field is first read whole.
    needing: Option<BTreeMap<RunId, Arc<FieldRow>>>,
    /// Of those the field holds, the one that has waited longest on the person among those
    /// answered here.
    longest: Option<RunId>,
}

/// What a piece of news can turn to need the person once it is folded.
pub(super) enum Moved {
    /// Every run kept: a field read whole, which calls nobody.
    Field,
    /// A run read whole as it is opened, which stands as read and calls nobody.
    Read(RunId),
    /// The runs whose standing it can move, a promotion's successor among them.
    Runs(Vec<RunId>),
}

impl Moved {
    /// What `news` can turn to need the person once it is folded.
    pub(super) fn by(news: &FollowNews) -> Moved {
        let runs = match news {
            FollowNews::Field(_) => return Moved::Field,
            FollowNews::Whole { run, .. } => return Moved::Read(run.run.clone()),
            FollowNews::Elsewhere(_) => Vec::new(),
            FollowNews::Run { run, news } => match news {
                RunNews::Events(_) | RunNews::Engine(_) => vec![run.clone()],
                RunNews::Successor { run: by, .. } => vec![run.clone(), by.run.clone()],
                RunNews::Said { .. }
                | RunNews::NoSuccessor
                | RunNews::Awaited(_)
                | RunNews::AwaitedUnread { .. }
                | RunNews::Document { .. }
                | RunNews::Remark(_) => Vec::new(),
            },
            FollowNews::Joined(run) => vec![run.run.clone()],
            FollowNews::Unreadable(why) => vec![why.run_id.clone()],
            FollowNews::Left(run) => vec![run.clone()],
        };
        Moved::Runs(runs)
    }
}

impl Calling {
    /// What the news that `moved` folded into `kept` asks at `now`: the person called once
    /// for each run it turned to need them. What is read whole calls nobody, and a field
    /// read whole sets which runs need them from then on.
    pub(super) fn folded(
        &mut self,
        moved: Moved,
        kept: &FollowedRuns,
        now: DateTime<Utc>,
    ) -> Vec<Request> {
        let (runs, calling) = match moved {
            Moved::Field => {
                let field = kept.field.iter().flat_map(|field| field.runs.keys());
                let in_view = kept.in_view.iter().map(|in_view| &in_view.run.run);
                let needing = field.chain(in_view).filter_map(|run| {
                    let row = needing_row(kept, run, now)?;
                    Some((run.clone(), row))
                });
                self.needing = Some(needing.collect());
                self.longest = self.waiting_longest(kept);
                return Vec::new();
            }
            Moved::Read(run) => (vec![run], false),
            Moved::Runs(runs) => (runs, true),
        };
        let Some(needing) = self.needing.as_mut() else {
            return Vec::new();
        };
        let needed = runs.iter().any(|run| needing.contains_key(run));
        let turned = runs.iter().filter(|run| turned(needing, run, kept, now));
        let turned = turned.count();
        // Which run has waited longest moves only with a run that needs the person, or
        // needed them before.
        if needed || turned > 0 {
            self.longest = self.waiting_longest(kept);
        }
        match calling {
            true => vec![Request::CallThePerson; turned],
            false => Vec::new(),
        }
    }

    /// The window's title as `kept` stands, `glyphs` drawing its mark: the run in view's
    /// where it needs the person, else that of the field's run that has waited longest on
    /// them among those answered here; none where no run needs them.
    pub(super) fn title(&self, kept: &FollowedRuns, glyphs: Glyphs) -> Option<String> {
        let needing = self.needing.as_ref()?;
        let in_view = kept.in_view.as_deref();
        let in_view = in_view.and_then(|in_view| needing.get(&in_view.run.run));
        let in_the_field = |run: &&RunId| {
            let field = kept.field.as_ref();
            field.is_some_and(|field| field.runs.contains_key(*run))
        };
        let longest = || needing.get(self.longest.as_ref().filter(in_the_field)?);
        let row = in_view.or_else(longest)?;
        Some(watched::title(&row.frame, &row.frame.run_id, glyphs))
    }

    /// Of the runs that need the person and the field holds, the one that has waited
    /// longest on them among those answered here: a run asking at its own terminal is
    /// answered there.
    fn waiting_longest(&self, kept: &FollowedRuns) -> Option<RunId> {
        let field = kept.field.as_ref()?;
        let needing = self.needing.iter().flatten();
        let held = needing.filter(|(run, _)| field.runs.contains_key(*run));
        let longest = waiting_longest(held.map(|(_, row)| row.as_ref()))?;
        Some(longest.frame.run_id.clone())
    }
}

/// Whether `run` turned to need the person at `now`, as `kept` keeps it, where `needing`
/// held the runs that needed them before: `needing` holds it from then on, as its row
/// stands now, where it needs them, and lets it go where it does not.
fn turned(
    needing: &mut BTreeMap<RunId, Arc<FieldRow>>,
    run: &RunId,
    kept: &FollowedRuns,
    now: DateTime<Utc>,
) -> bool {
    match needing_row(kept, run, now) {
        Some(row) => needing.insert(run.clone(), row).is_none(),
        None => {
            needing.remove(run);
            false
        }
    }
}

/// `run`'s row at `now`, as `kept` keeps it, where it stands needing the person: the
/// field's where the field holds it, the run in view's otherwise; none where neither does.
fn needing_row(kept: &FollowedRuns, run: &RunId, now: DateTime<Utc>) -> Option<Arc<FieldRow>> {
    let row = held(kept, run)?.row(now);
    (row.frame.standing == Standing::NeedsYou).then(|| Arc::new(row))
}

/// `run` as `kept` holds it: the field's where the field holds it, the run in view
/// otherwise, where it is that run.
fn held<'a>(kept: &'a FollowedRuns, run: &RunId) -> Option<&'a FollowedRun> {
    let field = kept.field.as_ref().and_then(|field| field.runs.get(run));
    let in_view = || {
        let in_view = kept.in_view.as_deref().map(|in_view| &in_view.run);
        in_view.filter(|held| held.run == *run)
    };
    field.map(Arc::as_ref).or_else(in_view)
}
