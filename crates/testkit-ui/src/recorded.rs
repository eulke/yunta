//! Each kind of operation the application asks for, performed once in each place of
//! the home where it reads something different, and what it found and came to: what
//! the walk answers an operation of that kind with, in every state that waits on one.
//! The acts that change a run are performed last, once every reading is made.
//!
//! What the application follows is recorded by place, `followed`'s, as a state opened there
//! follows it: the field at each reach in the places the walk's field comes from, and each
//! run whose screen it opens, its performer's following taken piece by piece until the state
//! stands caught up with the place's log, before any act moves a log. Each piece is kept by
//! what it is about, the field at the reach followed or the run it names at the head its log
//! stands at there, and the walk hands it to every state that follows that and holds the run
//! at that head, or none of it yet. The fold takes an event once and a piece handed again
//! changes nothing, so what is followed adds no endless states: it bounds the walk as the
//! recorded answers do.
//!
//! The runs of the home's cuts, each standing at a scene of a run's screen, are followed on
//! their screens, and the application is opened on each of their screens: what those screens
//! ask to read as a scene opens, a pull request or a verification, is recorded there.

mod followed;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use yunta_api::{FollowNews, Repository, RunRequest, Workspace};
use yunta_core::events::Channel;
use yunta_core::step::NextStep;
use yunta_core::{
    Answer, InputName, NodeId, OptionId, PackRef, QuestionId, WorkflowName, WorkflowShape,
};
use yunta_ui::{
    EvidenceOperation, EvidenceOutcome, Input, News, Operation, OperationKind, Outcome,
    PackOperation, PackOutcome, Performer, Request, State, WorkflowOperation,
};

use crate::cut::Cut;
use crate::home::{root_of, Home};
use crate::sandbox::{now, workspace, workspace_on};
use crate::walk::Reached;
use followed::{read_as, FollowedNews};

/// What each kind of operation found and came to where it was performed, what the
/// application's following handed where it was recorded, and the steps the application
/// is also opened on, whose openings they answer.
pub struct Recordings {
    kinds: Vec<(OperationKind, Recorded)>,
    followed: FollowedNews,
    opened_on: Vec<NextStep>,
}

/// What operations of one kind found while they lasted, and what they came to.
#[derive(Default)]
struct Recorded {
    news: Vec<News>,
    outcomes: Vec<Outcome>,
}

impl Recordings {
    /// Every kind of operation the application asks for, performed in `home`: every
    /// reading first, then the acts that change a run.
    pub async fn made(home: &Home) -> Recordings {
        let reviewed = NextStep::Review {
            run: home.reviewing.run_id.clone(),
        };
        let asked = NextStep::status(&home.asking.run_id);
        let choosing = NextStep::status(&home.scenes.choosing.run_id);
        let mut opened_on = vec![reviewed, asked, choosing];
        for run in home.scenes.cuts().flat_map(Cut::runs) {
            let status = NextStep::status(run);
            if !opened_on.contains(&status) {
                opened_on.push(status);
            }
        }
        let mut made = Recordings {
            kinds: Vec::new(),
            followed: FollowedNews::default(),
            opened_on,
        };
        made.readings(home).await;
        made.acts(home).await;
        made
    }

    /// The steps the application is also opened on, beside its opening: `review` of the
    /// home's run parked on its plan, the screen of each of its runs parked on a round, typed
    /// or of choices, which no field the walk shows lists and whose round stands in place
    /// there, and the screen of each run of the home's cuts, once for each run however many
    /// moments it stands at.
    pub fn opened_on(&self) -> &[NextStep] {
        &self.opened_on
    }

    /// Every reading the application asks for, performed where it reads something
    /// different; what it follows first, while the home's own root holds no run.
    async fn readings(&mut self, home: &Home) {
        self.followings(home).await;
        self.repository_readings(home).await;
        self.catalog_readings(home).await;
        self.finished_readings(home).await;
        self.run_readings(home).await;
        self.machine_readings(home).await;
    }

    /// Where the application opens, read in each place it opens differently, and
    /// what setting a repository up finds, with an adapter answering.
    async fn repository_readings(&mut self, home: &Home) {
        let (outside, never_set_up) = (home.outside.as_path(), home.never_set_up.as_path());
        let unworked = home.root();
        for dir in [outside, never_set_up, home.set_up.as_path()] {
            self.kept(performed(dir, unworked, Operation::ReadRepository).await);
        }
        let answering = workspace_on(never_set_up, unworked, vec![home.clis.clone()]);
        self.kept(performed_in(answering, Operation::DetectProject).await);
    }

    /// The catalog, a workflow's check, past runs (none) and graph, the cases, and the
    /// packs: none installed, and one installed and audited.
    async fn catalog_readings(&mut self, home: &Home) {
        let (unworked, set_up) = (home.root(), home.set_up.as_path());
        self.kept(performed(set_up, unworked, Operation::ReadCatalog).await);
        let greet = WorkflowOperation::Check {
            workflow: "greet".to_string(),
        };
        self.kept(performed(set_up, unworked, Operation::Workflow(greet)).await);
        let never_ran = Operation::ReadHistory {
            workflow: WorkflowName::from("greet"),
        };
        self.kept(performed(set_up, unworked, never_ran).await);
        let drawn = Operation::ReadGraph {
            workflow: "greet".to_string(),
        };
        self.kept(performed(set_up, unworked, drawn).await);
        let cases = Operation::Workflow(WorkflowOperation::RunCases);
        self.kept(performed(set_up, unworked, cases).await);
        let packed = home.packed.as_path();
        for dir in [set_up, packed] {
            let listing = Operation::Pack(PackOperation::List);
            self.kept(performed(dir, unworked, listing).await);
        }
        let demo = "acme/demo".parse::<PackRef>().expect("a pack reference");
        let audit = Operation::Pack(PackOperation::Audit { pack: demo.clone() });
        self.kept(performed(packed, unworked, audit).await);
        let tools = home.tools.to_string_lossy().to_string();
        let fetch = Operation::Pack(PackOperation::Fetch { source: tools });
        self.kept(performed(set_up, unworked, fetch).await);
        let update = PackOperation::FetchUpdate {
            pack: demo,
            reference: String::new(),
        };
        self.kept(performed(packed, unworked, Operation::Pack(update)).await);
    }

    /// What a finished run leaves, and its workflow's past runs.
    async fn finished_readings(&mut self, home: &Home) {
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let finished = home.worked.run_id.clone();
        let past = Operation::ReadHistory {
            workflow: WorkflowName::from("done"),
        };
        self.kept(performed(worked.0, worked.1, past).await);
        for evidence in [
            EvidenceOperation::WriteReceipt {
                run: finished.clone(),
            },
            EvidenceOperation::Verify {
                run: finished.clone(),
            },
            EvidenceOperation::WritePageForBrowser { run: finished },
        ] {
            let evidence = Operation::Evidence(evidence);
            self.kept(performed(worked.0, worked.1, evidence).await);
        }
    }

    /// What doctor finds of a repository set up, with no session and with a session per
    /// binding, none of which answers here; and what collecting what its runs left would
    /// reclaim, and reclaims.
    async fn machine_readings(&mut self, home: &Home) {
        let (unworked, set_up) = (home.root(), home.set_up.as_path());
        for sessions in [false, true] {
            let checks = Operation::Doctor { sessions };
            self.kept(performed(set_up, unworked, checks).await);
        }
        for dry_run in [true, false] {
            let collect = Operation::Collect { dry_run };
            self.kept(performed(set_up, unworked, collect).await);
        }
    }

    /// The pull request of the gate a run waits at. The home holds no run waiting at a gate a
    /// forge holds: the pull request of the run parked on its own gate is read as none. What
    /// a run's log says, the decision and the round it waits on among it, is followed.
    async fn run_readings(&mut self, home: &Home) {
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let no_pull_request = Operation::ReadPullRequest {
            run: home.parked.clone(),
        };
        self.kept(performed(worked.0, worked.1, no_pull_request).await);
    }

    /// Every act the application asks for, performed on a run it changes or a
    /// repository it sets up or writes into.
    async fn acts(&mut self, home: &Home) {
        let answering = workspace_on(&home.to_set_up, home.root(), vec![home.clis.clone()]);
        self.kept(set_up(answering).await);
        self.catalog_acts(home).await;
        self.run_acts(home).await;
    }

    /// A new workflow and a new pack, each named and named by nothing, and the pack
    /// installed removed.
    async fn catalog_acts(&mut self, home: &Home) {
        for name in ["", "fresh"] {
            let new = WorkflowOperation::New {
                name: name.to_string(),
                shape: WorkflowShape::OneNode,
            };
            self.kept(performed(&home.set_up, home.root(), Operation::Workflow(new)).await);
        }
        for pack in ["", "acme/fresh"] {
            let new = PackOperation::Scaffold {
                pack: pack.to_string(),
            };
            self.kept(performed(&home.set_up, home.root(), Operation::Pack(new)).await);
        }
        let demo = "acme/demo".parse::<PackRef>().expect("a pack reference");
        let removal = Operation::Pack(PackOperation::Remove { pack: demo });
        self.kept(performed(&home.packed, home.root(), removal).await);
        let source = home.tools.to_string_lossy().to_string();
        self.kept(installed(workspace(&home.set_up, home.root()), source).await);
    }

    /// A run started, decided, answered, resumed, cancelled and closed.
    async fn run_acts(&mut self, home: &Home) {
        let start = Operation::Start {
            request: RunRequest {
                workflow: PathBuf::from("greet"),
                inputs: HashMap::from([(InputName::from("ask"), "greet ana".to_string())]),
                mode: None,
                adapter: None,
                hermetic: false,
            },
        };
        self.kept(performed(&home.set_up, home.root(), start).await);
        let decide = Operation::Decide {
            run: home.deciding.run_id.clone(),
            option: OptionId::from("go"),
            words: None,
        };
        let deciding = (home.deciding.worktree.as_path(), root_of(&home.deciding));
        self.kept(performed(deciding.0, deciding.1, decide).await);
        let answer = Operation::Answer {
            run: home.asking.run_id.clone(),
            node: NodeId::from("ask"),
            answers: vec![Answer {
                id: QuestionId::from("summary"),
                value: "the greeting".to_string(),
            }],
        };
        let asking = (home.asking.worktree.as_path(), root_of(&home.asking));
        self.kept(performed(asking.0, asking.1, answer).await);
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let run = home.parked.clone();
        for act in [
            Operation::Resume { run: run.clone() },
            Operation::Cancel { run: run.clone() },
            Operation::Close { runs: vec![run] },
        ] {
            self.kept(performed(worked.0, worked.1, act).await);
        }
    }

    /// What operations of `operation`'s kind came to, each place one was performed;
    /// nothing for a kind never recorded.
    pub fn of(&self, operation: &Operation) -> &[Outcome] {
        self.recorded(operation)
            .map_or(&[], |recorded| recorded.outcomes.as_slice())
    }

    /// What operations of `operation`'s kind found while they lasted, each place one
    /// was performed.
    pub fn news_of(&self, operation: &Operation) -> &[News] {
        self.recorded(operation)
            .map_or(&[], |recorded| recorded.news.as_slice())
    }

    /// What the application's following handed of what `state` follows, where it was
    /// recorded: each piece of the field at its reach, then each piece of its run recorded
    /// where the run's log stood at the head `state` holds it read whole at, or every
    /// place's where it holds none of it.
    pub fn followed(&self, state: &State) -> Vec<&FollowNews> {
        let followed = state.followed();
        let in_view = state.followed_runs().in_view.as_ref();
        let held = in_view
            .filter(|in_view| followed.run.as_ref() == Some(&in_view.run.run))
            .and_then(|in_view| in_view.run.state.head());
        self.followed.about(&followed, held)
    }

    /// Every page a piece of work wrote for a surface to show: what the walk tells each
    /// state a surface could not show.
    pub fn pages_written(&self) -> Vec<PathBuf> {
        self.kinds
            .iter()
            .flat_map(|(_, recorded)| &recorded.outcomes)
            .filter_map(|outcome| match outcome {
                Outcome::Evidence(EvidenceOutcome::PageWritten { page }) => Some(page.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every piece of work `requests` asks for, and everything it asks to follow, is one
    /// these recordings answer, which the walk answers it with; `reached` says the inputs
    /// that asked where one is not.
    pub fn carried_out(&self, requests: &[Request], reached: &Reached<'_>) {
        for request in requests {
            match request {
                Request::Perform(_, operation) => assert!(
                    !self.of(operation).is_empty(),
                    "{operation:?} has no recording in recorded.rs; asked after:\n{}",
                    reached.inputs()
                ),
                Request::Follow(followed) => assert!(
                    self.followed.answers(followed),
                    "{followed:?} has no following recorded in recorded.rs; asked after:\n{}",
                    reached.inputs()
                ),
                Request::LetGo(_)
                | Request::ShowPage(_)
                | Request::CallThePerson
                | Request::Leave => {}
            }
        }
    }

    fn recorded(&self, operation: &Operation) -> Option<&Recorded> {
        let kind = operation.kind();
        self.kinds
            .iter()
            .find(|(recorded, _)| *recorded == kind)
            .map(|(_, recorded)| recorded)
    }

    /// What `operation` found and came to, kept under the operation's kind.
    fn kept(&mut self, (operation, news, outcome): (Operation, Vec<News>, Outcome)) {
        let kind = operation.kind();
        let at = match self
            .kinds
            .iter()
            .position(|(recorded, _)| *recorded == kind)
        {
            Some(at) => at,
            None => {
                self.kinds.push((kind, Recorded::default()));
                self.kinds.len() - 1
            }
        };
        let recorded = &mut self.kinds[at].1;
        recorded.news.extend(news);
        recorded.outcomes.push(outcome);
    }
}

/// The repository `workspace` works in set up as the application sets it up, by one
/// performer: the project detected under the ticket the application asks it under,
/// then written with every answer left as detected, under another ticket of the
/// application's own.
async fn set_up(workspace: Workspace) -> (Operation, Vec<News>, Outcome) {
    let dir = workspace.dir.clone();
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(opening, _)] = requests.as_slice() else {
        panic!("the application asks where it stands as it opens");
    };
    let never_set_up = read_as(&workspace, Repository::NeverSetUp);
    let requests = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(detecting, Operation::DetectProject)] = requests.as_slice() else {
        panic!("a repository never set up opens on finding what it runs");
    };
    let performer = Performer::new(workspace, Channel::Tty);
    let (inputs, mut received) = mpsc::unbounded_channel();
    performer
        .perform(*detecting, Operation::DetectProject, inputs.clone())
        .await;
    let draft = loop {
        match received.recv().await {
            Some(Input::Outcome(_, Outcome::Detected { draft, .. })) => break draft,
            Some(Input::Outcome(_, other)) => panic!("{dir:?} is not set up: {other:?}"),
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
    };
    let writing = Operation::WriteSetup {
        held: *detecting,
        setup: draft.answered("", "", Vec::new()),
    };
    performer.perform(*opening, writing.clone(), inputs).await;
    loop {
        match received.recv().await {
            Some(Input::Outcome(_, outcome)) => return (writing, Vec::new(), outcome),
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
    }
}

/// The pack `source` names installed as the application installs it, by one
/// performer: fetched under a ticket of the application's own, then installed from
/// the copy held under it, its executors confirmed, under another.
async fn installed(workspace: Workspace, source: String) -> (Operation, Vec<News>, Outcome) {
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(opening, _)] = requests.as_slice() else {
        panic!("the application asks where it stands as it opens");
    };
    let never_set_up = read_as(&workspace, Repository::NeverSetUp);
    let requests = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(fetching, _)] = requests.as_slice() else {
        panic!("a repository never set up asks what setting it up finds");
    };
    let performer = Performer::new(workspace, Channel::Tty);
    let (inputs, mut received) = mpsc::unbounded_channel();
    let fetch = Operation::Pack(PackOperation::Fetch { source });
    performer.perform(*fetching, fetch, inputs.clone()).await;
    let held = loop {
        match received.recv().await {
            Some(Input::Outcome(_, Outcome::Pack(PackOutcome::Fetched(fetched)))) => {
                break fetched.held
            }
            Some(Input::Outcome(_, other)) => panic!("the pack is not fetched: {other:?}"),
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
    };
    let install = Operation::Pack(PackOperation::Install {
        held,
        confirmed: true,
    });
    performer.perform(*opening, install.clone(), inputs).await;
    loop {
        match received.recv().await {
            Some(Input::Outcome(_, outcome)) => return (install, Vec::new(), outcome),
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
    }
}

/// `operation`, what it found while it lasted and what it came to, performed by a
/// performer working in `dir` with its state under `root`.
async fn performed(
    dir: &Path,
    root: &Path,
    operation: Operation,
) -> (Operation, Vec<News>, Outcome) {
    performed_in(workspace(dir, root), operation).await
}

/// `operation`, what it found while it lasted and what it came to, performed by a
/// performer working through `workspace`.
async fn performed_in(
    workspace: Workspace,
    operation: Operation,
) -> (Operation, Vec<News>, Outcome) {
    // A ticket of the application's own: the performer hands back under what it is given.
    let (_, requests) = State::opened(now());
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
    let (inputs, mut received) = mpsc::unbounded_channel();
    Performer::new(workspace, Channel::Tty)
        .perform(ticket, operation.clone(), inputs)
        .await;
    let mut news = Vec::new();
    loop {
        match received.recv().await {
            Some(Input::News(_, found)) => news.push(found),
            Some(Input::Outcome(_, outcome)) => return (operation, news, outcome),
            Some(_) => continue,
            None => panic!("the performer handed back no outcome"),
        }
    }
}
