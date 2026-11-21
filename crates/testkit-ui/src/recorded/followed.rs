//! See [`super`]. What the application follows, recorded in the places the walk's field and
//! runs come from: a state opened there with a performer, its following's news taken until
//! it stands caught up with the place's log, each piece kept by what it is about. A state
//! opened as though its repository stood otherwise than it does is told the place's own
//! store and prices with it (`read_as`), as the recordings of a setup are.
//!
//! A piece about a run is kept under the head the run's log stands at in the place it was
//! recorded in, since one run stands in several places at several heads, the design's run at
//! each of its moments: a state that holds the run read whole is handed only the pieces of
//! the place it was read in, so it never takes for one run's news what another place's log
//! says of it; a state that holds none of it yet is handed every place's, and comes to
//! stand at each.
//!
//! The runs whose screens the walk opens on a scene stand in places of their own, the cuts
//! of the home, whose fields are no field the walk shows: there only what is about a run is
//! kept, beside what the run's screen asks to read as its scene opens (`on_their_screens`).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use yunta_api::{FieldReach, FollowNews, Followed, Repository, Workspace};
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::{RunId, Seq};
use yunta_ui::prompt::Stroke;
use yunta_ui::{
    Input, News, Operation, Outcome, Performer, RepositoryReading, Request, State, Ticket,
};

use super::Recordings;
use crate::home::{root_of, Home};
use crate::sandbox::{now, workspace};

/// How long a state takes at most to stand caught up with a place's log: its following
/// looks a tenth of a second apart and reads what a run waits on beside, and a place's
/// work is a reading or two, so only a state that never stands waits half a minute.
const STANDING: Duration = Duration::from_secs(30);

/// What the application's following handed where it was recorded, each piece by what it
/// is about: the field, by the reach it was followed at, or the run it names, by the head
/// that run's log stood at where it was recorded.
#[derive(Default)]
pub(super) struct FollowedNews {
    fields: Vec<(FieldReach, Vec<FollowNews>)>,
    runs: BTreeMap<RunId, BTreeMap<Seq, Vec<FollowNews>>>,
}

impl FollowedNews {
    /// Each piece about what `followed` names: of the field at its reach, then of its run,
    /// those recorded where its log stood at `held`, the head a state holds it read whole
    /// at, or every place's where it holds none of it.
    pub(super) fn about(&self, followed: &Followed, held: Option<Seq>) -> Vec<&FollowNews> {
        let field = self
            .fields
            .iter()
            .filter(|(reach, _)| followed.field == Some(*reach))
            .flat_map(|(_, news)| news);
        let run = followed
            .run
            .iter()
            .filter_map(|run| self.runs.get(run))
            .flat_map(|places| places.iter())
            .filter(|(head, _)| held.is_none_or(|held| held == **head))
            .flat_map(|(_, news)| news);
        field.chain(run).collect()
    }

    /// `run` recorded as followed, whatever its following handed: a successor read whole
    /// with its predecessor's news is followed on, and its following hands nothing of it
    /// again.
    fn followed_on(&mut self, run: RunId) {
        self.runs.entry(run).or_default();
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

    /// `news`, taken in a place whose log stands at `heads` by a state following the field
    /// at `reach`, kept under what it is about: a piece of the field under that reach, and a
    /// piece of a run under that run and its head there. A piece of a field followed at no
    /// reach is dropped, as the state drops a field it no longer follows.
    fn kept(&mut self, reach: Option<FieldReach>, news: FollowNews, heads: &BTreeMap<RunId, Seq>) {
        let run = match &news {
            FollowNews::Field(_)
            | FollowNews::Joined(_)
            | FollowNews::Unreadable(_)
            | FollowNews::Left(_)
            | FollowNews::Elsewhere(_) => None,
            FollowNews::Whole { run, .. } => Some(run.run.clone()),
            FollowNews::Run { run, .. } => Some(run.clone()),
        };
        match (run, reach) {
            (Some(run), _) => {
                let head = *heads
                    .get(&run)
                    .unwrap_or_else(|| panic!("{run:?}, which is followed, stands in the log"));
                let places = self.runs.entry(run).or_default();
                places.entry(head).or_default().push(news);
            }
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
    /// run whose screen it opens, as a state opened there follows it. A field is followed
    /// only in a repository set up: in a place never set up, the state is told its
    /// repository is set up, as the walk answers its opening, and follows the field there:
    /// here, where no run stands and its first request does; here and then at the machine's
    /// reach, where the field stands. Each run is followed from the application opened on
    /// following it: one in a repository never set up stands over its setup, the asking
    /// runs, the typed round's and the round of choices', each in its own place, and the reviewing run over its field, which it goes back to, its
    /// prompt in place put aside first, and follows at the machine's reach. Then each run of
    /// the home's cuts, on its screen in its cut.
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
        for run in [&home.worked.run_id, &home.parked] {
            self.followed_alone(worked.clone(), &at_worked, run).await;
        }
        for asking in [&home.asking, &home.scenes.choosing] {
            let place = workspace(&asking.worktree, root_of(asking));
            let at_asking = yunta_testkit::heads(asking.store_path());
            self.followed_alone(place, &at_asking, &asking.run_id).await;
        }
        let reviewing = &home.reviewing;
        let over_its_field = Followed {
            field: Some(FieldReach::Here),
            run: Some(reviewing.run_id.clone()),
        };
        let back_and_further = [
            (None, over_its_field.clone()),
            (Some(Stroke::Decline), over_its_field),
            (Some(Stroke::Decline), here),
            (further, machine),
        ];
        let opened = opened_following(&reviewing.run_id);
        let place = workspace(&reviewing.worktree, root_of(reviewing));
        let at_reviewing = yunta_testkit::heads(reviewing.store_path());
        self.followed_through(place, &at_reviewing, opened, &back_and_further)
            .await;
        for cut in home.scenes.cuts() {
            self.on_their_screens(cut.workspace(), cut.runs()).await;
        }
    }

    /// What the following of `run` alone hands, the application opened on its screen in
    /// `place`, whose log stands at `heads`.
    async fn followed_alone(
        &mut self,
        place: Workspace,
        heads: &BTreeMap<RunId, Seq>,
        run: &RunId,
    ) {
        let alone = Followed {
            field: None,
            run: Some(run.clone()),
        };
        let opened = opened_following(run);
        self.followed_through(place, heads, opened, &[(None, alone)])
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
                    Input::FollowNews(news) => {
                        let reach = state.followed().field;
                        self.followed.kept(reach, news.clone(), heads);
                    }
                    Input::LookFailed(why) => panic!("a look at what is followed failed: {why}"),
                    _ => {}
                }
                asked = state.update(input);
            }
        }
    }

    /// What the following hands of each run of `runs`, the application opened on the run's
    /// screen in `place`, and what each screen asks to read as its scene opens there, a
    /// pull request read or a run's events verified: each piece about a run kept under the
    /// run and its head here, those of the run its screen moves onto, a child or a
    /// successor, among them; each reading kept under its kind. Pieces of the field are
    /// left out, and the reading of where the application stands as it opens, which no
    /// scene asks. Each screen is taken until it waits on nothing and stands caught up with
    /// the place's log; the run it then follows is recorded as followed, whatever its
    /// following handed.
    pub(super) async fn on_their_screens(&mut self, place: Workspace, runs: &[RunId]) {
        let project = place
            .project()
            .expect("the place's project names its store");
        let heads = yunta_testkit::heads(&project.storage_path);
        for run in runs {
            let performer = Arc::new(Performer::new(place.clone(), Channel::Tty));
            let (inputs, mut received) = mpsc::unbounded_channel();
            let (mut state, opening) = opened_following(run);
            for request in opening {
                performer.take(request, &inputs);
            }
            let mut asked = Vec::new();
            let mut readings = Readings::default();
            loop {
                for request in asked.drain(..) {
                    readings.asked(&request);
                    performer.take(request, &inputs);
                }
                if state.waiting_on().is_empty() && state.caught_up(&heads) {
                    break;
                }
                let came = tokio::time::timeout(STANDING, received.recv()).await;
                let input = came
                    .unwrap_or_else(|_| {
                        panic!(
                            "{run:?}'s screen never stands caught up at {heads:?}: it follows \
                             {:?} and waits on {:?}",
                            state.followed(),
                            state.waiting_on()
                        )
                    })
                    .expect("the performer hands back what it reads");
                match &input {
                    Input::FollowNews(news) => self.followed.kept(None, news.clone(), &heads),
                    Input::LookFailed(why) => panic!("a look at what is followed failed: {why}"),
                    other => {
                        if let Some(read) = readings.came(other) {
                            self.kept(read);
                        }
                    }
                }
                asked = state.update(input);
            }
            if let Some(run) = state.followed().run {
                self.followed.followed_on(run);
            }
        }
    }
}

/// The readings a run's screen asked as its scene opened, each under the ticket it was asked
/// under, with what it found so far.
#[derive(Default)]
struct Readings {
    asked: BTreeMap<Ticket, (Operation, Vec<News>)>,
}

impl Readings {
    /// `request`, kept where it asks for a reading.
    fn asked(&mut self, request: &Request) {
        if let Request::Perform(ticket, operation) = request {
            self.asked.insert(*ticket, (operation.clone(), Vec::new()));
        }
    }

    /// `input`, folded into the reading it answers: what it found kept, and the reading
    /// handed back with all it found once it came to its outcome.
    fn came(&mut self, input: &Input) -> Option<(Operation, Vec<News>, Outcome)> {
        match input {
            Input::News(ticket, news) => {
                let (_, found) = self.asked.get_mut(ticket)?;
                found.push(news.clone());
                None
            }
            Input::Outcome(ticket, outcome) => {
                let (operation, found) = self.asked.remove(ticket)?;
                Some((operation, found, outcome.clone()))
            }
            _ => None,
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

/// The application opened on `run`'s screen, which follows the run, and what it asks first.
fn opened_following(run: &RunId) -> (State, Vec<Request>) {
    let status = NextStep::status(run);
    State::opened_on(now(), &status).expect("the run's stage takes its status")
}
