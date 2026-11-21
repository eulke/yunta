//! The pull request of a gate a run waits at, polled through the API on the forge the run's
//! config declares, as a run's screen reads it: the pull request the gate published, by its
//! number and its link, and what the forge answers of it, read again on each poll; why no
//! forge answered, the pull request still said, where none reaches it here; and none for a
//! run that waits on no pull request.

mod bench_project;

use std::sync::Arc;

use yunta_adapters::{MockForge, MockForgeState};
use yunta_api::ForgeSource;
use yunta_core::port::{Forge, ForgeUnanswered, ReviewOutcome};
use yunta_core::{ConfigLayer, SecretSource};
use yunta_testkit::{Bench, MOCK_CONFIG};

use bench_project::project_of;

/// A change, then an external gate on a pull request of it.
const PUBLISHES: &str = r#"
name: published
nodes:
  - id: make
    kind: bash
    run: "echo made > made.txt"
  - id: approve
    kind: gate
    depends_on: [make]
    assignee: reviewer
    external:
      kind: pull_request
      artifacts: []
      branch: "{{run.branch}}"
"#;

/// A gate nobody answers at the console.
const APPROVING: &str = r#"
name: approving
nodes:
  - id: ship
    kind: gate
    assignee: the release lead
"#;

/// No session: no node of the runs opens one.
const NO_SESSIONS: &str = "dialect: claude-code\nsessions: []\n";

/// The forge the runs declare, whose token is in a variable no test sets.
const FORGE: &str =
    "forge:\n  github: { repo: acme/web, token_env: YUNTA_TEST_UNSET_FORGE_TOKEN }\n";

/// Where the API reaches a forge in these tests: the mock forge a run published on,
/// standing in for every forge a config declares.
struct OnTheMock(MockForgeState);

impl ForgeSource for OnTheMock {
    fn forge(
        &self,
        config: &ConfigLayer,
        _secrets: &dyn SecretSource,
    ) -> Result<Arc<dyn Forge>, ForgeUnanswered> {
        match config
            .forge
            .as_ref()
            .and_then(|forge| forge.github.as_ref())
        {
            Some(_) => Ok(Arc::new(MockForge::new(self.0.clone()))),
            None => Err(ForgeUnanswered::Undeclared),
        }
    }
}

/// A run parked on its gate's pull request on `state`'s forge, under the id `id`.
async fn parked_on_its_pull_request(id: &str, state: &MockForgeState) -> Bench {
    let bench = Bench::with_run_id(id).with_forge(Arc::new(MockForge::new(state.clone())));
    let config = format!("{MOCK_CONFIG}{FORGE}");
    bench.run_with_config(PUBLISHES, NO_SESSIONS, &config).await;
    bench
}

#[tokio::test]
async fn a_gates_pull_request_is_polled_on_the_forge_its_run_declares_at_each_poll() {
    let state = MockForgeState::new();
    let bench = parked_on_its_pull_request("polled-pull-request", &state).await;
    let mut project = project_of(&bench);
    project.workspace.forges = Arc::new(OnTheMock(state.clone()));
    let open = project.run(&bench.run_id).await.unwrap();
    let polled = open.pull_request().await.unwrap();
    let polled = polled.expect("the run waits on its gate's pull request");
    assert!(
        polled
            .pull_request
            .url
            .ends_with(&format!("/{}", polled.pull_request.number)),
        "{polled:?}"
    );
    let answered = polled.answered.as_ref().expect("the forge answered");
    assert_eq!(answered.review, ReviewOutcome::Pending);
    state.approve(bench.run_id.as_str(), &"ana".into());
    let polled = open
        .pull_request()
        .await
        .unwrap()
        .expect("its pull request");
    let answered = polled.answered.expect("the forge answered");
    let ReviewOutcome::Approved { by, reviewed_sha } = answered.review else {
        panic!("the forge answers the approval: {:?}", answered.review);
    };
    assert_eq!(by.to_string(), "ana");
    assert_eq!(
        reviewed_sha, answered.head_sha,
        "it approved the last commit"
    );
}

#[tokio::test]
async fn where_no_forge_reaches_it_the_pull_request_is_said_with_why() {
    let state = MockForgeState::new();
    let bench = parked_on_its_pull_request("unreached-pull-request", &state).await;
    let project = project_of(&bench);
    let open = project.run(&bench.run_id).await.unwrap();
    let polled = open.pull_request().await.unwrap();
    let polled = polled.expect("the run waits on its gate's pull request");
    let unset = ForgeUnanswered::TokenUnset {
        token_env: "YUNTA_TEST_UNSET_FORGE_TOKEN".to_string(),
    };
    assert_eq!(polled.answered, Err(unset));
}

#[tokio::test]
async fn a_run_that_waits_on_no_pull_request_has_none_to_poll() {
    let bench = Bench::with_run_id("no-pull-request");
    bench.run(APPROVING, NO_SESSIONS).await;
    let mut project = project_of(&bench);
    project.workspace.forges = Arc::new(OnTheMock(MockForgeState::new()));
    let open = project.run(&bench.run_id).await.unwrap();
    assert_eq!(open.pull_request().await.unwrap(), None);
}
