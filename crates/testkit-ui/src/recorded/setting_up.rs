//! See [`super`]. A repository's setup as the application asks for its work, by one performer:
//! the project detected, the pack a setup installs fetched for it, then the setup written with
//! every answer left, the pack installed or not.

use std::sync::Arc;

use yunta_api::{Repository, Workspace};
use yunta_core::events::Channel;
use yunta_ui::{
    Input, InstallsThePack, News, Operation, Outcome, Performer, Request, SetupPackFetch, State,
    Ticket,
};

use super::followed::read_as;
use super::performed_by;
use crate::sandbox::now;

/// A repository's setup as the application asks for it, by one performer, of which a setup
/// is made: the project detected and the pack a setup installs fetched, each under the
/// ticket the application asks it under as the setup opens, the pack held with the project.
struct SettingUp {
    performer: Arc<Performer>,
    /// The ticket the project is held under.
    held: Ticket,
    /// The ticket the application asked where it stands under, which nothing waits on
    /// once it is answered.
    opening: Ticket,
    /// What the detection and the fetch came to, in that order.
    done: [(Operation, Vec<News>, Outcome); 2],
}

/// What setting the repository `workspace` works in up finds and what fetching the pack a
/// setup installs comes to there, as the application asks for each.
pub(super) async fn found_and_fetched(
    workspace: Workspace,
) -> [(Operation, Vec<News>, Outcome); 2] {
    setting_up(workspace).await.done
}

/// The repository `workspace` works in, its setup asked for as the application asks for it,
/// up to its pack's fetch.
async fn setting_up(workspace: Workspace) -> SettingUp {
    let dir = workspace.dir.clone();
    let (mut state, requests) = State::opened(now());
    let [Request::Perform(opening, _)] = requests.as_slice() else {
        panic!("the application asks where it stands as it opens");
    };
    let never_set_up = read_as(&workspace, Repository::NeverSetUp { dir: dir.clone() });
    let requests = state.update(Input::Outcome(*opening, never_set_up));
    let [Request::Perform(detecting, Operation::DetectProject), Request::Perform(fetching, Operation::FetchSetupPack), Request::Follow(_)] =
        requests.as_slice()
    else {
        panic!(
            "{dir:?}: a repository never set up opens on finding what it runs and fetching its \
             pack, following its field: {requests:?}"
        );
    };
    let performer = Arc::new(Performer::new(workspace, Channel::Tty));
    let found = performed_by(&performer, *detecting, Operation::DetectProject).await;
    let fetched = performed_by(&performer, *fetching, Operation::FetchSetupPack).await;
    SettingUp {
        performer,
        held: *detecting,
        opening: *opening,
        done: [found, fetched],
    }
}

/// The repository `workspace` works in set up as the application sets it up, by one
/// performer: the project detected and the pack a setup installs fetched for it, then
/// written with every answer left, installing the pack where `installs` says so, the
/// project then proposing its roles, under the ticket the application asked where it
/// stands under.
pub(super) async fn set_up(
    workspace: Workspace,
    installs: InstallsThePack,
) -> (Operation, Vec<News>, Outcome) {
    let setting_up = setting_up(workspace).await;
    let [(_, _, Outcome::Detected { draft, .. }), (_, _, Outcome::SetupPack { fetched, .. })] =
        &setting_up.done
    else {
        panic!("not set up: {:?}", setting_up.done);
    };
    let mut answered = (**draft).clone();
    if let (InstallsThePack::Yes, SetupPackFetch::Fetched { proposed, .. }) = (installs, &**fetched)
    {
        answered.proposed = proposed.clone();
    }
    let writing = Operation::WriteSetup {
        held: setting_up.held,
        setup: answered.answered("", "", Vec::new()),
        pack: installs,
    };
    performed_by(&setting_up.performer, setting_up.opening, writing).await
}
