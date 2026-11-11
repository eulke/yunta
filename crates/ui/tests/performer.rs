//! The performer, against a workspace a test makes in a temporary directory: what an
//! operation comes to arrives as an input under its ticket, and what failed below
//! Yunta comes back in its own words. It reads nothing of its process.

mod support;

use tokio::sync::mpsc;
use yunta_api::Repository;
use yunta_api::Workspace;
use yunta_ui::{Input, Operation, Outcome, Performer, Request, State};

use support::{drive, now, workspace};

/// What the performer hands back for the operation the opening state asks.
async fn opened_in(workspace: Workspace) -> Outcome {
    let (_, requests) = State::opened(now());
    let [Request::Perform(ticket, operation)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert_eq!(*operation, Operation::ReadRepository);
    let (inputs, mut received) = mpsc::unbounded_channel();
    Performer::new(workspace)
        .perform(*ticket, operation.clone(), inputs)
        .await;
    loop {
        match received.recv().await.expect("an outcome") {
            Input::Outcome(answered, outcome) => {
                assert_eq!(answered, *ticket, "under the ticket it was asked under");
                return outcome;
            }
            Input::Remark(..) => continue,
            other => panic!("{other:?}"),
        }
    }
}

/// The repository the application works in is answered as far as Yunta goes: none
/// outside one, never set up in a repository with no config of Yunta's, and set up
/// in one that has it.
#[tokio::test]
async fn the_repository_it_works_in_is_answered() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let answered = opened_in(workspace(&outside, root.path())).await;
    assert!(
        matches!(answered, Outcome::Repository(Repository::Absent)),
        "{answered:?}"
    );

    let repo = root.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    yunta_testkit::init_repo(&repo);
    let answered = opened_in(workspace(&repo, root.path())).await;
    assert!(
        matches!(answered, Outcome::Repository(Repository::NeverSetUp)),
        "{answered:?}"
    );

    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    std::fs::write(repo.join(".yunta/config.yaml"), "").unwrap();
    let answered = opened_in(workspace(&repo, root.path())).await;
    assert!(
        matches!(answered, Outcome::Repository(Repository::SetUp)),
        "{answered:?}"
    );
}

#[tokio::test]
async fn what_failed_below_yunta_comes_back_in_its_own_words() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    std::fs::create_dir_all(repo.join(".yunta")).unwrap();
    yunta_testkit::init_repo(&repo);
    std::fs::write(
        repo.join(".yunta/config.yaml"),
        "runners: [not, a, mapping\n",
    )
    .unwrap();
    let answered = opened_in(workspace(&repo, root.path())).await;
    let Outcome::Failed(said) = answered else {
        panic!("{answered:?}");
    };
    assert!(said.contains("config.yaml"), "{said}");
}

/// Driven outside a repository, the application comes to rest on the signpost, waiting
/// on nothing, and leaves at `q`.
#[tokio::test]
async fn the_signpost_is_driven_to_quiet() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let performer = std::sync::Arc::new(Performer::new(workspace(&outside, root.path())));
    let (opened, asked) = State::opened(now());
    let strokes = [yunta_ui::prompt::Stroke::Insert('q')];
    let (state, surfaced) = drive(opened, asked, &performer, &strokes).await;
    assert!(state.waiting_on().is_empty());
    let screen = surco::surface::Markdown.screen(&state.screen(&surco::surface::Markdown.look()));
    assert!(screen.contains("not in a repository"), "{screen}");
    assert_eq!(surfaced, [Request::Leave]);
}
