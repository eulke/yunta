//! The forge a real invocation can offer, built from the config and the
//! secrets the process was given.

use std::sync::Arc;

use yunta_adapters::GitHubForge;
use yunta_core::port::Forge;
use yunta_core::{describe, ConfigLayer, NodeKind, SecretSource, Workflow};

use crate::error::{warn, CliError};

/// The forge a real invocation can offer — `None` when either
/// `forge.github` isn't configured, or the named `token_env` isn't
/// bound in `secrets`, in which case a gate degrades to the console
/// instead. `yunta check` already refuses a workflow with an external
/// gate when the former is missing; the latter is a legitimate,
/// expected runtime state — person B's machine, with no credentials at
/// all, still runs `yunta` just fine, it only ever falls back to the
/// console for a gate it can't reach the forge for.
pub(crate) fn forge_for(
    config: &ConfigLayer,
    secrets: &dyn SecretSource,
) -> Option<Arc<dyn Forge>> {
    let github = config.forge.as_ref()?.github.as_ref()?;
    let token = secrets.get(&github.token_env)?;
    match GitHubForge::new(github.repo.clone(), token) {
        Ok(forge) => Some(Arc::new(forge)),
        Err(e) => {
            warn(format!(
                "the forge is unavailable — {}; external gates degrade to the console",
                describe(&e)
            ));
            None
        }
    }
}

/// Refuses a run that would open a pull request through a forge this
/// machine cannot reach: the config declares one, and the variable its
/// token is in is not set here. The node would fail when the run reaches
/// it, after every node before it spent; a `pull_request` node the run
/// leaves out asks nothing.
pub(crate) fn refuse_unreachable_forge(
    config: &ConfigLayer,
    workflow: &Workflow,
    secrets: &dyn SecretSource,
) -> Result<(), CliError> {
    let Some(github) = config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
    else {
        return Ok(());
    };
    let left_out: Vec<yunta_core::NodeId> = yunta_core::left_out(workflow, config)
        .into_iter()
        .map(|left| left.node)
        .collect();
    let opens = workflow.iter_nodes_with_group().any(|(node, group)| {
        matches!(node.kind, NodeKind::PullRequest { .. })
            && !left_out.contains(group.map_or(&node.id, |group| &group.id))
    });
    if !opens || secrets.get(&github.token_env).is_some() {
        return Ok(());
    }
    Err(CliError::msg(format!(
        "this workflow opens a pull request through `forge.github` ({}), and `{}`, the \
         variable its token is in, is not set here — set it, then run again",
        github.repo, github.token_env
    )))
}

/// What `doctor` says about the forge the config declares — nothing
/// when it declares none: whether its token is set here, whether the
/// repository answers it and lets it push, and whether the remote a run
/// pushes to is that repository. `false` when any of it would stop a
/// `pull_request` node.
pub(crate) async fn report_forge(ctx: &crate::context::Context) -> bool {
    let config = &ctx.project.config;
    let Some(github) = config
        .forge
        .as_ref()
        .and_then(|forge| forge.github.as_ref())
    else {
        return true;
    };
    let named = format!("forge: github {}", github.repo);
    let Some(forge) = forge_for(config, &yunta_core::ProcessSecrets) else {
        println!(
            "{named} — `{}`, the variable its token is in, is not set",
            github.token_env
        );
        return false;
    };
    let healthy = match forge.probe().await {
        Ok(yunta_core::port::ForgeProbe {
            can_push: Some(false),
        }) => {
            println!("{named} — reachable, and its token cannot push there");
            false
        }
        Ok(_) => {
            println!("{named} — reachable");
            true
        }
        Err(error) => {
            println!("{named} — {}", describe(&error));
            false
        }
    };
    let remote = github.remote();
    match yunta_engine::git::remote_url(&ctx.cwd, remote, ctx.supervision()).await {
        Some(url) if url.contains(&github.repo.to_string()) => healthy,
        Some(url) => {
            println!("  remote `{remote}` is {url}, not {}", github.repo);
            false
        }
        None => {
            println!("  no remote `{remote}` to push a run's branch to");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yunta_core::{ConfigLayer, Secret, SecretSource};

    use super::forge_for;

    struct Bound(BTreeMap<&'static str, &'static str>);

    impl SecretSource for Bound {
        fn get(&self, name: &str) -> Option<Secret<String>> {
            self.0
                .get(name)
                .map(|value| Secret::from(value.to_string()))
        }
    }

    fn github_config() -> ConfigLayer {
        yunta_core::yaml::parse("forge:\n  github: { repo: acme/web, token_env: ACME_TOKEN }\n")
            .expect("a forge config")
    }

    #[test]
    fn forge_for_is_none_without_the_token_it_names() {
        let elsewhere = Bound(BTreeMap::from([("OTHER_TOKEN", "t")]));
        assert!(forge_for(&github_config(), &elsewhere).is_none());
    }

    #[test]
    fn forge_for_builds_the_forge_the_named_token_reaches() {
        let bound = Bound(BTreeMap::from([("ACME_TOKEN", "t")]));
        assert!(forge_for(&github_config(), &bound).is_some());
    }
}
