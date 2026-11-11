//! Each kind of operation the application asks for, performed once in each place of
//! the home where it reads something different, and what it found and came to: what
//! the walk answers an operation of that kind with, in every state that waits on one.
//! The acts that change a run are performed last, once every reading is made.

use std::collections::HashMap;
use std::mem::{discriminant, Discriminant};
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use yunta_api::{FieldReach, RunRequest};
use yunta_core::{Answer, InputName, NodeId, OptionId, QuestionId};
use yunta_ui::{Input, News, Operation, Outcome, Performer, Request, State};

use super::home::{root_of, Home};
use crate::support::{now, workspace};

/// What each kind of operation found and came to where it was performed.
pub struct Recordings(Vec<(Discriminant<Operation>, Recorded)>);

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
        let (outside, never_set_up) = (home.outside.as_path(), home.never_set_up.as_path());
        let worked = (home.worked.worktree.as_path(), home.worked_root());
        let unworked = home.root();
        let set_up = home.set_up.as_path();
        for dir in [outside, never_set_up, set_up] {
            self.kept(performed(dir, unworked, Operation::ReadRepository).await);
        }
        for reach in [FieldReach::Here, FieldReach::Machine] {
            self.kept(performed(never_set_up, unworked, Operation::Field { reach }).await);
            self.kept(performed(worked.0, worked.1, Operation::Field { reach }).await);
        }
        self.kept(performed(set_up, unworked, Operation::ReadCatalog).await);
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

    /// Every act the application asks for, performed on a run it changes.
    async fn acts(&mut self, home: &Home) {
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

    fn recorded(&self, operation: &Operation) -> Option<&Recorded> {
        let kind = discriminant(operation);
        self.0
            .iter()
            .find(|(recorded, _)| *recorded == kind)
            .map(|(_, recorded)| recorded)
    }

    /// What `operation` found and came to, kept under the operation's kind.
    fn kept(&mut self, (operation, news, outcome): (Operation, Vec<News>, Outcome)) {
        let kind = discriminant(&operation);
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

/// `operation`, what it found while it lasted and what it came to, performed by a
/// performer working in `dir` with its state under `root`.
async fn performed(
    dir: &Path,
    root: &Path,
    operation: Operation,
) -> (Operation, Vec<News>, Outcome) {
    // A ticket of the application's own: the performer hands back under what it is given.
    let (_, requests) = State::opened(now());
    let Some(Request::Perform(ticket, _)) = requests.into_iter().next() else {
        panic!("the application asks something as it opens");
    };
    let (inputs, mut received) = mpsc::unbounded_channel();
    Performer::new(workspace(dir, root))
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
