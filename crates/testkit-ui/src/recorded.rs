//! Each kind of operation the application asks for, performed once in each place of
//! the home where it reads something different, and what it found and came to: what
//! the walk answers an operation of that kind with, in every state that waits on one.
//! The acts that change a run are performed last, once every reading is made.

use std::collections::HashMap;

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use yunta_api::{FieldReach, Repository, RunRequest, Workspace};
use yunta_core::{
    Answer, InputName, NodeId, OptionId, PackRef, QuestionId, WorkflowName, WorkflowShape,
};
use yunta_ui::{
    EvidenceOperation, Input, News, Operation, OperationKind, Outcome, PackOperation, PackOutcome,
    Performer, Request, State, WorkflowOperation,
};

use crate::home::{root_of, Home};
use crate::sandbox::{now, workspace, workspace_on};
use crate::walk::Reached;

/// What each kind of operation found and came to where it was performed.
pub struct Recordings(Vec<(OperationKind, Recorded)>);

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
        let mut made = Recordings(Vec::new());
        made.readings(home).await;
        made.acts(home).await;
        made
    }

    /// Every reading the application asks for, performed where it reads something
    /// different.
    async fn readings(&mut self, home: &Home) {
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
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let unworked = home.root();
        for dir in [outside, never_set_up, home.set_up.as_path()] {
            self.kept(performed(dir, unworked, Operation::ReadRepository).await);
        }
        let answering = workspace_on(never_set_up, unworked, vec![home.clis.clone()]);
        self.kept(performed_in(answering, Operation::DetectProject).await);
        for reach in [FieldReach::Here, FieldReach::Machine] {
            self.kept(performed(never_set_up, unworked, Operation::Field { reach }).await);
            self.kept(performed(worked.0, worked.1, Operation::Field { reach }).await);
        }
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

    /// What a finished run leaves, cost and is drawn as, and its workflow's past runs.
    async fn finished_readings(&mut self, home: &Home) {
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let finished = home.worked.run_id.clone();
        for reading in [
            Operation::ReadStats {
                run: finished.clone(),
            },
            Operation::ReadRunGraph {
                run: finished.clone(),
            },
            Operation::ReadHistory {
                workflow: WorkflowName::from("done"),
            },
        ] {
            self.kept(performed(worked.0, worked.1, reading).await);
        }
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

    /// A run's decision and rounds, followed, and its pages.
    async fn run_readings(&mut self, home: &Home) {
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let parked = home.parked.clone();
        let deciding = Operation::ReadDecision {
            run: parked.clone(),
        };
        self.kept(performed(worked.0, worked.1, deciding).await);
        let no_round = Operation::ReadQuestions { run: parked };
        self.kept(performed(worked.0, worked.1, no_round).await);
        let asking = (home.asking.worktree.as_path(), root_of(&home.asking));
        let round = Operation::ReadQuestions {
            run: home.asking.run_id.clone(),
        };
        self.kept(performed(asking.0, asking.1, round).await);
        for (run, node) in [
            (home.worked.run_id.clone(), "only"),
            (home.parked.clone(), "approve"),
        ] {
            for reading in [
                Operation::Follow { run: run.clone() },
                Operation::ReadPage { run: run.clone() },
                Operation::ReadNodePage {
                    run,
                    node: NodeId::from(node),
                },
            ] {
                self.kept(performed(worked.0, worked.1, reading).await);
            }
        }
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

    /// Every piece of work `requests` asks for is one these recordings answer, which the
    /// walk answers it with; `reached` says the inputs that asked where one is not.
    pub fn carried_out(&self, requests: &[Request], reached: &Reached<'_>) {
        for request in requests {
            if let Request::Perform(_, operation) = request {
                assert!(
                    !self.of(operation).is_empty(),
                    "{operation:?} has no recording in recorded.rs; asked after:\n{}",
                    reached.inputs()
                );
            }
        }
    }

    fn recorded(&self, operation: &Operation) -> Option<&Recorded> {
        let kind = operation.kind();
        self.0
            .iter()
            .find(|(recorded, _)| *recorded == kind)
            .map(|(_, recorded)| recorded)
    }

    /// What `operation` found and came to, kept under the operation's kind.
    fn kept(&mut self, (operation, news, outcome): (Operation, Vec<News>, Outcome)) {
        let kind = operation.kind();
        let at = match self.0.iter().position(|(recorded, _)| *recorded == kind) {
            Some(at) => at,
            None => {
                self.0.push((kind, Recorded::default()));
                self.0.len() - 1
            }
        };
        let recorded = &mut self.0[at].1;
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
    let never_set_up = Outcome::Repository(Repository::NeverSetUp);
    let requests = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(detecting, Operation::DetectProject)] = requests.as_slice() else {
        panic!("a repository never set up opens on finding what it runs");
    };
    let performer = Performer::new(workspace);
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
    let never_set_up = Outcome::Repository(Repository::NeverSetUp);
    let requests = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(fetching, _)] = requests.as_slice() else {
        panic!("a repository never set up asks what setting it up finds");
    };
    let performer = Performer::new(workspace);
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
    Performer::new(workspace)
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
