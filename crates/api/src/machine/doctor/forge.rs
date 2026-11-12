//! See [`super`]. What `doctor` finds of the forge the config declares:
//! whether it answers and its token can push, and whether this checkout's
//! remote is the repository it names.

use yunta_core::describe;
use yunta_render::page::doctor::{Finding, ForgeAnswer, RemoteAnswer};

use crate::{Error, Project, Remarks, Stop};

/// What `doctor` finds of the forge the config declares. Nothing for a
/// config that declares none. A remote lookup the caller's stop ended is the
/// stop, not a finding about the remote.
pub(super) async fn findings(
    project: &Project,
    remarks: &dyn Remarks,
    stop: &Stop,
) -> Result<Vec<Finding>, Error> {
    let config = &project.config;
    let Some(github) = config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
    else {
        return Ok(Vec::new());
    };
    let repo = github.repo.clone();
    let secrets = project.workspace.secrets.as_ref();
    let Some(forge) = crate::forge::forge_for(config, secrets, remarks) else {
        return Ok(vec![Finding::ForgeTokenUnset {
            repo,
            token_env: github.token_env.clone(),
            opening: opening_a_pull_request(project),
        }]);
    };
    let answer = match forge.probe().await {
        Ok(yunta_core::port::ForgeProbe {
            can_push: Some(false),
        }) => ForgeAnswer::CannotPush,
        Ok(_) => ForgeAnswer::Reachable,
        Err(error) => ForgeAnswer::Unanswered {
            said: describe(&error),
        },
    };
    let mut findings = vec![Finding::Forge {
        repo: repo.clone(),
        answer,
    }];
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
    findings.extend(answer.map(|answer| Finding::Remote {
        remote: remote.to_string(),
        repo,
        answer,
    }));
    Ok(findings)
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
