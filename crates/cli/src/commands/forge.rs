//! The forge a real invocation can offer, built from the config and the
//! secrets the process was given.

use std::sync::Arc;

use yunta_adapters::GitHubForge;
use yunta_core::port::Forge;
use yunta_core::{describe, ConfigLayer, SecretSource};

use crate::error::warn;

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
