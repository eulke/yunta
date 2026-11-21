//! See [`super`]. What `doctor` finds of the forge the config declares:
//! whether it answers and its token can push, and whether this checkout's
//! remote is the repository it names.

use yunta_core::describe;
use yunta_core::port::ForgeUnanswered;
use yunta_core::GitHubForgeConfig;
use yunta_render::page::doctor::{Finding, ForgeAnswer, RemoteAnswer};

use crate::{Error, Project, Stop};

/// What `doctor` finds of the forge the config declares. Nothing for a
/// config that declares none. A remote lookup the caller's stop ended is the
/// stop, not a finding about the remote.
pub(super) async fn findings(project: &Project, stop: &Stop) -> Result<Vec<Finding>, Error> {
    let config = &project.config;
    let Some(github) = config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
    else {
        return Ok(Vec::new());
    };
    let repo = github.repo.clone();
    let answer = match project.workspace.forge(config) {
        Ok(forge) => match forge.probe().await {
            Ok(yunta_core::port::ForgeProbe {
                can_push: Some(false),
            }) => ForgeAnswer::CannotPush,
            Ok(_) => ForgeAnswer::Reachable,
            Err(error) => ForgeAnswer::Unanswered {
                said: describe(&error),
            },
        },
        Err(ForgeUnanswered::Failed { said }) => ForgeAnswer::Unready {
            token_env: github.token_env.clone(),
            said,
        },
        Err(ForgeUnanswered::Undeclared | ForgeUnanswered::TokenUnset { .. }) => {
            return Ok(vec![Finding::ForgeTokenUnset {
                repo,
                token_env: github.token_env.clone(),
                opening: opening_a_pull_request(project),
            }]);
        }
    };
    let forge = Finding::Forge {
        repo: repo.clone(),
        answer,
    };
    let remote = remote_finding(project, github, stop).await?;
    Ok(std::iter::once(forge).chain(remote).collect())
}

/// Whether the remote this checkout pushes a run's branch to is the repository `github`
/// names: nothing where it is, what it is or why it could not be read where it is not. A
/// lookup the caller's stop ended is the stop.
async fn remote_finding(
    project: &Project,
    github: &GitHubForgeConfig,
    stop: &Stop,
) -> Result<Option<Finding>, Error> {
    let remote = github.remote();
    let dir = &project.workspace.dir;
    let supervision = project.workspace.supervision(stop);
    let answer = match yunta_engine::git::remote_url(dir, remote, supervision).await {
        Ok(Some(url)) if github.repo.is_named_by(&url) => None,
        Ok(Some(url)) => Some(RemoteAnswer::Elsewhere { url }),
        Ok(None) => Some(RemoteAnswer::Missing),
        Err(stopped) if stopped.cancelled() => return Err(Error::Stopped),
        Err(unread) => Some(RemoteAnswer::Unread {
            detail: unread.detail(),
        }),
    };
    Ok(answer.map(|answer| Finding::Remote {
        remote: remote.to_string(),
        repo: github.repo.clone(),
        answer,
    }))
}

/// The catalog workflows that open a pull request through the forge: what
/// makes a token that is not set a problem, rather than a gate falling back
/// to the console.
fn opening_a_pull_request(project: &Project) -> Vec<String> {
    let config = &project.config;
    project
        .workspace
        .catalog_workflows()
        .into_iter()
        .filter(|(_, workflow)| yunta_core::opens_a_pull_request(workflow, config))
        .map(|(name, _)| name)
        .collect()
}
