//! See [`super`]. What the application follows, recorded in the places the walk's field and
//! runs come from: a state opened there with a performer, its following's news taken until
//! it stands caught up with the place's log, each piece kept by what it is about. A state
//! opened as though its repository stood otherwise than it does is told the place's own
//! store and prices with it (`read_as`), as the recordings of a setup are.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::{FieldReach, FollowNews, Followed, Repository, Workspace};
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::{RunId, Seq};
use yunta_ui::prompt::Stroke;
use yunta_ui::{Input, Outcome, Performer, RepositoryReading, Request, State};

use super::Recordings;
use crate::home::{root_of, Home};
use crate::sandbox::{now, workspace};

/// How long a state takes at most to stand caught up with a place's log: its following
/// looks a tenth of a second apart and reads what a run waits on beside, and a place's
/// work is a reading or two, so only a state that never stands waits half a minute.
const STANDING: Duration = Duration::from_secs(30);

/// What the application's following handed where it was recorded, each piece by what it
/// is about: the field, by the reach it was followed at, or the run it names.
#[derive(Default)]
pub(super) struct FollowedNews {
    fields: Vec<(FieldReach, Vec<FollowNews>)>,
    runs: BTreeMap<RunId, Vec<FollowNews>>,
}

impl FollowedNews {
    /// Each piece about what `followed` names: of the field at its reach, then of its run.
    pub(super) fn about(&self, followed: &Followed) -> Vec<&FollowNews> {
        let field = self
            .fields
            .iter()
            .filter(|(reach, _)| followed.field == Some(*reach))
            .flat_map(|(_, news)| news);
        let run = followed
            .run
            .iter()
            .filter_map(|run| self.runs.get(run))
            .flatten();
        field.chain(run).collect()
    }

    /// Whether each thing `followed` names was recorded: the field at its reach and the
    /// run.
    pub(super) fn answers(&self, followed: &Followed) -> bool {
        let field = followed
            .field
            .is_none_or(|reach| self.fields.iter().any(|(at, _)| *at == reach));
        let run = followed
            .run
            .as_ref()
            .is_none_or(|run| self.runs.contains_key(run));
        field && run
    }

    /// `news`, taken by a state following `followed`, kept under what it is about: a
    /// piece of the field under the reach followed, and a piece of a run under that run.
    /// A piece of a field no longer followed is dropped, as the state drops it.
    fn kept(&mut self, followed: &Followed, news: FollowNews) {
        let run = match &news {
            FollowNews::Field(_)
            | FollowNews::Joined(_)
            | FollowNews::Unreadable(_)
            | FollowNews::Left(_)
            | FollowNews::Elsewhere(_) => None,
            FollowNews::Whole { run, .. } => Some(run.run.clone()),
            FollowNews::Run { run, .. } => Some(run.clone()),
        };
        match (run, followed.field) {
            (Some(run), _) => self.runs.entry(run).or_default().push(news),
            (None, Some(reach)) => match self.fields.iter_mut().find(|(at, _)| *at == reach) {
                Some((_, kept)) => kept.push(news),
                None => self.fields.push((reach, vec![news])),
            },
            (None, None) => {}
        }
    }
}

impl Recordings {
    /// What the application follows in the places the walk's field comes from, and of each
    /// run whose page it reads, as a state opened there follows it. A field is followed
    /// only in a repository set up: in a place never set up, the state is told its
    /// repository is set up, as the walk answers its opening, and follows the field there:
    /// here, where no run stands and its first request does; here and then at the machine's
    /// reach, where the field stands. Each run is followed from the application opened on
    /// following it: one in a repository never set up stands over its setup, and the
    /// reviewing run over its field, which it goes back to and follows at the machine's
    /// reach.
    pub(super) async fn followings(&mut self, home: &Home) {
        let here = Followed {
            field: Some(FieldReach::Here),
            run: None,
        };
        let machine = Followed {
            field: Some(FieldReach::Machine),
            run: None,
        };
        let further = Some(Stroke::Insert('a'));
        let never_set_up = workspace(&home.never_set_up, home.root());
        // The home's own root holds no run until the acts start one.
        let empty = BTreeMap::new();
        let field_here = [(None, here.clone())];
        let opened = told_set_up(&never_set_up);
        self.followed_through(never_set_up, &empty, opened, &field_here)
            .await;
        let worked = workspace(&home.worked.worktree, home.worked_root());
        let at_worked = yunta_testkit::heads(home.worked.store_path());
        let each_reach = [(None, here.clone()), (further, machine.clone())];
        let opened = told_set_up(&worked);
        self.followed_through(worked.clone(), &at_worked, opened, &each_reach)
            .await;
        for run in [home.worked.run_id.clone(), home.parked.clone()] {
            let opened = opened_following(&run);
            let alone = Followed {
                field: None,
                run: Some(run),
            };
            self.followed_through(worked.clone(), &at_worked, opened, &[(None, alone)])
                .await;
        }
        let reviewing = &home.reviewing;
        let over_its_field = Followed {
            field: Some(FieldReach::Here),
            run: Some(reviewing.run_id.clone()),
        };
        let back_and_further = [
            (None, over_its_field),
            (Some(Stroke::Decline), here),
            (further, machine),
        ];
        let opened = opened_following(&reviewing.run_id);
        let place = workspace(&reviewing.worktree, root_of(reviewing));
        self.followed_through(
            place,
            &yunta_testkit::heads(reviewing.store_path()),
            opened,
            &back_and_further,
        )
        .await;
    }

    /// What the following of `state`, just opened and asking `asked`, hands, its work
    /// carried out by a performer working through `workspace`: each piece kept under what
    /// the state followed as it took it. At each of `steps` its stroke is pressed, where it
    /// has one, and pieces are taken until the state follows what the step names, stands
    /// caught up with `heads`, each run's last seq in the place's log, and waits on
    /// nothing: where the next stroke finds the stage it is for.
    async fn followed_through(
        &mut self,
        workspace: Workspace,
        heads: &BTreeMap<RunId, Seq>,
        (mut state, mut asked): (State, Vec<Request>),
        steps: &[(Option<Stroke>, Followed)],
    ) {
        let performer = Arc::new(Performer::new(workspace, Channel::Tty));
        let (inputs, mut received) = mpsc::unbounded_channel();
        for (stroke, followed) in steps {
            if let Some(stroke) = stroke {
                asked = state.update(Input::Stroke(*stroke));
            }
            loop {
                for request in asked.drain(..) {
                    // What only a surface does, showing a page or leaving, comes back:
                    // nothing here shows a page, and no step leaves.
                    performer.take(request, &inputs);
                }
                let at_rest = state.waiting_on().is_empty();
                if at_rest && state.followed() == *followed && state.caught_up(heads) {
                    break;
                }
                let came = tokio::time::timeout(STANDING, received.recv()).await;
                let input = came
                    .unwrap_or_else(|_| {
                        panic!(
                            "the state never stands following {followed:?} at {heads:?}: it \
                             follows {:?} and waits on {:?}",
                            state.followed(),
                            state.waiting_on()
                        )
                    })
                    .expect("the performer hands back what it reads");
                match &input {
                    Input::FollowNews(news) => self.followed.kept(&state.followed(), news.clone()),
                    Input::LookFailed(why) => panic!("a look at what is followed failed: {why}"),
                    _ => {}
                }
                asked = state.update(input);
            }
        }
    }
}

/// The repository `workspace` works in, read as `repository` whatever it is, with where
/// its project keeps its runs and its prices as the performer reads them: what a state is
/// told where the recordings open it in a place as though it stood elsewhere.
pub(super) fn read_as(workspace: &Workspace, repository: Repository) -> Outcome {
    let project = workspace.project();
    let project = project.expect("the place's project names its store and its prices");
    Outcome::Repository(Box::new(RepositoryReading {
        repository,
        storage: project.storage_path,
        prices: project.config.pricing,
    }))
}

/// The application opened, told the repository `workspace` works in is set up, and what
/// it asks then: the field here, which it follows from there.
fn told_set_up(workspace: &Workspace) -> (State, Vec<Request>) {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(opening, _)] = requests.as_slice() else {
        panic!("the application asks where it stands as it opens");
    };
    let set_up = read_as(workspace, Repository::SetUp);
    let requests = state.update(Input::Outcome(*opening, set_up));
    (state, requests)
}

/// The application opened on following `run`, and what it asks first.
fn opened_following(run: &RunId) -> (State, Vec<Request>) {
    let follow = NextStep::Follow { run: run.clone() };
    State::opened_on(now(), &follow).expect("the run's stage takes following it")
}
