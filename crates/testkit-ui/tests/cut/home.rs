//! Every place of the home a target opens in holds what the target shows: the workflows,
//! modes and past runs a new run reads, what admission refuses there, the runs that need
//! the person, the pack a setup installs in reach or out of it, and who git commits as.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use yunta_api::{Committer, Error, FieldReach, Remark, Repository, RunRequest, Stop, Workspace};
use yunta_core::{InputName, NodeKind, Workflow};
use yunta_replay::frame::RunPhase;
use yunta_testkit::reference_pack::{VERSION_TAG, YUNTA_REPOSITORY};
use yunta_testkit_ui::cut::{Stands, TARGET_CUTS};
use yunta_testkit_ui::home::Home;

/// What a new run's targets type as the request.
const REQUEST: &str = "add support for installing a pack globally";

/// What a place of the home holds for a target that opens there.
#[derive(Debug, Clone, Copy)]
enum Need {
    /// A workflow of several modes: the modes a new run marks one of.
    Modes,
    /// A workflow that reads an input beside its request: the rows a new run asks.
    InputBesideTheRequest,
    /// That workflow, asked for with its request alone, refused by admission for the input
    /// left empty: what a new run says in place.
    Refused,
    /// A workflow of several nodes that stops for a person: the graph a person reads.
    Graph,
    /// Three finished runs of one workflow, each with its own spend and time: what a new
    /// run of it usually takes.
    PastRuns,
    /// Runs that need the person: what the tally over a new run counts.
    NeedsYou,
    /// A repository never set up, two adapters answering, whose git reaches the pack a
    /// setup installs at the Yunta repository's URL, under the binary's version.
    PackInReach,
    /// A repository never set up whose git reaches nothing at the Yunta repository's URL.
    PackOutOfReach,
    /// A repository whose git commits as a person, by the name and the address a person
    /// gives git: what the machine's facts say of who commits.
    CommitsByName,
}

/// What each target that opens in a place of the home needs that place to hold.
const NEEDS: &[(&str, &[Need])] = &[
    (
        "new-run",
        &[
            Need::Modes,
            Need::PastRuns,
            Need::NeedsYou,
            Need::CommitsByName,
        ],
    ),
    ("new-run-mode", &[Need::Modes]),
    ("new-run-inputs", &[Need::InputBesideTheRequest]),
    (
        "new-run-refused",
        &[Need::InputBesideTheRequest, Need::Refused],
    ),
    ("workflow-graph", &[Need::Graph]),
    ("workflow-history", &[Need::PastRuns]),
    ("setup", &[Need::PackInReach, Need::CommitsByName]),
    ("setup-changing", &[Need::PackInReach]),
    ("setup-audit", &[Need::PackInReach]),
    ("setup-confirm", &[Need::PackInReach]),
    ("setup-written", &[Need::PackInReach]),
    ("setup-unreachable", &[Need::PackOutOfReach]),
];

/// On a deep stack: the home's places run whole runs inside the test, which a debug
/// build's futures make deeper than a test thread's stack.
#[test]
fn every_place_of_the_home_a_target_opens_in_holds_what_the_target_needs() {
    yunta_testkit::on_a_deep_stack(|| async {
        let home = Home::built().await;
        for (name, needs) in NEEDS {
            let target = TARGET_CUTS
                .iter()
                .find(|target| target.name == *name)
                .unwrap_or_else(|| panic!("no target is named `{name}`"));
            let Stands::Home(place) = target.stands else {
                panic!("`{name}` opens on a cut: it needs a place of the home holding {needs:?}");
            };
            let workspace = home.workspace_at(place);
            for need in *needs {
                holds(&workspace, *need, name).await;
            }
        }
    });
}

/// Asserts that the place `workspace` opens in holds `need`, which `target` needs.
async fn holds(workspace: &Workspace, need: Need, target: &str) {
    let catalog = workspace.catalog_workflows();
    match need {
        Need::Modes => {
            let modes = catalog
                .iter()
                .any(|(_, workflow)| workflow.modes.as_ref().is_some_and(|modes| modes.len() > 1));
            assert!(modes, "`{target}`: no workflow of several modes");
        }
        Need::InputBesideTheRequest => {
            assert!(
                beside_the_request(&catalog).is_some(),
                "`{target}`: no workflow reads an input beside its request"
            );
        }
        Need::Refused => refused(workspace, &catalog, target).await,
        Need::Graph => {
            let graph = catalog.iter().any(|(_, workflow)| {
                workflow.iter_nodes().count() > 1
                    && workflow
                        .iter_nodes()
                        .any(|node| matches!(node.kind, NodeKind::Gate { .. }))
            });
            assert!(
                graph,
                "`{target}`: no workflow of several nodes stops for a person"
            );
        }
        Need::PastRuns => past_runs(workspace, &catalog, target).await,
        Need::NeedsYou => {
            let project = workspace.project().expect("the place's project");
            let field = project
                .field(FieldReach::Here, &Stop::new())
                .await
                .expect("the place's field");
            assert!(
                !field.needing_a_person().is_empty(),
                "`{target}`: no run needs the person"
            );
        }
        Need::PackInReach => pack_in_reach(workspace, target).await,
        Need::PackOutOfReach => pack_out_of_reach(workspace, target).await,
        Need::CommitsByName => commits_by_name(workspace, target).await,
    }
}

/// The name a repository a test makes commits as, which no person goes by.
const NOBODYS_NAME: &str = "Test";

/// Asserts that `workspace`'s git commits as a person: a name of two words and more, and
/// an address.
async fn commits_by_name(workspace: &Workspace, target: &str) {
    let committer = workspace
        .committer(&Stop::new())
        .await
        .expect("who git commits as");
    let Committer::Named(who) = committer else {
        panic!("`{target}`: git names nobody: {committer:?}");
    };
    let (name, address) = who
        .split_once(" <")
        .unwrap_or_else(|| panic!("`{target}`: `{who}` names no address"));
    assert!(
        name != NOBODYS_NAME && name.split_whitespace().count() >= 2 && address.contains('@'),
        "`{target}`: git commits as `{who}`, no name a person goes by"
    );
}

/// Asserts that the workflow of `catalog` with an input beside its request, asked for in
/// `workspace` with its request alone, is refused by admission for the input left empty.
async fn refused(workspace: &Workspace, catalog: &[(String, Workflow)], target: &str) {
    let (name, request, other) =
        beside_the_request(catalog).expect("a workflow with an input beside its request");
    let project = workspace.project().expect("the place's project");
    let asked = RunRequest {
        workflow: PathBuf::from(&name),
        inputs: HashMap::from([(request, REQUEST.to_string())]),
        mode: None,
        adapter: None,
        hermetic: false,
    };
    let remarks = |_: Remark| {};
    match project.admit(asked, &remarks, &Stop::new()).await {
        Err(Error::Refused(refused)) => {
            let said = format!("{refused:?}");
            assert!(
                said.contains("Missing") && said.contains(other.as_str()),
                "`{target}`: `{name}` is refused for another reason: {said}"
            );
        }
        Err(other) => panic!("`{target}`: `{name}` fails rather than being refused: {other}"),
        Ok(_) => panic!("`{target}`: `{name}` is admitted with `{other}` left empty"),
    }
}

/// Asserts that a workflow of `catalog` has three finished runs in `workspace`'s field,
/// each spending and taking its own.
async fn past_runs(workspace: &Workspace, catalog: &[(String, Workflow)], target: &str) {
    let project = workspace.project().expect("the place's project");
    let field = project
        .field(FieldReach::Here, &Stop::new())
        .await
        .expect("the place's field");
    let mut found = false;
    for (_, workflow) in catalog {
        let finished = field
            .rows
            .iter()
            .filter(|row| row.frame.workflow == workflow.name)
            .filter(|row| matches!(row.frame.phase, RunPhase::Finished))
            .count();
        let summaries = project.history(&workflow.name).await.summaries();
        let spent: BTreeSet<u64> = summaries.iter().map(|run| run.tokens).collect();
        let took: BTreeSet<_> = summaries.iter().filter_map(|run| run.wall_clock).collect();
        found |= finished >= 3 && spent.len() >= 3 && took.len() >= 3;
    }
    assert!(
        found,
        "`{target}`: no workflow has three finished runs, each its own"
    );
}

/// Asserts that `workspace`'s repository was never set up, that two adapters answer there,
/// and that its git reaches fragua at the Yunta repository's URL, under the binary's
/// version.
async fn pack_in_reach(workspace: &Workspace, target: &str) {
    never_set_up(workspace, target).await;
    let answering = yunta_api::machine::probe_known_adapters(&workspace.env.path)
        .await
        .into_iter()
        .filter(|adapter| adapter.healthy)
        .count();
    assert!(answering >= 2, "`{target}`: {answering} adapters answer");
    let clone = tempfile::tempdir().expect("a place for the clone");
    let cloned = fetched(workspace, clone.path(), target);
    assert!(
        cloned.status.success(),
        "`{target}`: {}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    let manifest = std::fs::read_to_string(clone.path().join("yunta/packs/fragua/pack.yaml"))
        .unwrap_or_else(|error| panic!("`{target}`: no fragua at the version tag: {error}"));
    assert!(
        manifest.contains("name: fragua") && manifest.contains("publisher: yunta"),
        "`{target}`: {manifest}"
    );
}

/// Asserts that `workspace`'s repository was never set up, and that its git reaches
/// nothing at the Yunta repository's URL.
async fn pack_out_of_reach(workspace: &Workspace, target: &str) {
    never_set_up(workspace, target).await;
    let clone = tempfile::tempdir().expect("a place for the clone");
    let cloned = fetched(workspace, clone.path(), target);
    assert!(
        !cloned.status.success(),
        "`{target}`: the Yunta repository is in reach"
    );
}

/// The workflow of `catalog` that reads its request and a required input beside it: its
/// name, its request's input and the other one.
fn beside_the_request(catalog: &[(String, Workflow)]) -> Option<(String, InputName, InputName)> {
    catalog.iter().find_map(|(name, workflow)| {
        let request = workflow
            .inputs
            .iter()
            .find(|(_, spec)| spec.is_request())?
            .0;
        let other = workflow
            .inputs
            .iter()
            .find(|(_, spec)| !spec.is_request() && spec.is_required())?
            .0;
        Some((name.to_string(), request.clone(), other.clone()))
    })
}

/// Asserts that `workspace`'s repository was never set up.
async fn never_set_up(workspace: &Workspace, target: &str) {
    let project = workspace.project().expect("the place's project");
    let repository = project
        .repository(&Stop::new())
        .await
        .expect("the place's repository");
    assert_eq!(repository, Repository::NeverSetUp, "`{target}`");
}

/// The Yunta repository cloned under `into` and checked out at the binary's version, by
/// the git `workspace` hands its processes: refused, without asking the network, where
/// that git does not point the Yunta repository's URL at a place of the test's.
fn fetched(workspace: &Workspace, into: &Path, target: &str) -> std::process::Output {
    let vars = &workspace.env.subprocess_vars;
    let pointed = vars
        .iter()
        .any(|(key, value)| key.starts_with("GIT_CONFIG_VALUE_") && value == YUNTA_REPOSITORY);
    assert!(
        pointed,
        "`{target}`: its git does not point the Yunta repository's URL anywhere"
    );
    let args = [
        "clone",
        "--quiet",
        "--branch",
        VERSION_TAG,
        YUNTA_REPOSITORY,
        "yunta",
    ];
    yunta_testkit::git_with(into, &args, vars)
}
