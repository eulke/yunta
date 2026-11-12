//! See [`crate`]. The forge a run under a config reaches, built from what the
//! config declares and the token the workspace's secrets hold: what a run
//! publishes a gate and opens a pull request through, and what `doctor`
//! probes.

use std::sync::Arc;

use yunta_core::port::Forge;
use yunta_core::{ConfigLayer, SecretSource};

use crate::{Remark, Remarks};

/// The forge a run under `config` reaches: none when the config declares
/// none, or the variable its token is in is not one `secrets` holds (a gate
/// then falls back to the person, a legitimate state on a machine with no
/// credentials); and none, said through `remarks`, when the forge does not
/// take the token.
pub(crate) fn forge_for(
    config: &ConfigLayer,
    secrets: &dyn SecretSource,
    remarks: &dyn Remarks,
) -> Option<Arc<dyn Forge>> {
    let github = config.forge.as_ref()?.github.as_ref()?;
    let token = secrets.get(&github.token_env)?;
    match yunta_adapters::GitHubForge::new(github.repo.clone(), token) {
        Ok(forge) => Some(Arc::new(forge)),
        Err(error) => {
            remarks.remark(Remark::ForgeUnavailable {
                said: yunta_core::describe(&error),
            });
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yunta_core::{ConfigLayer, Secret, SecretSource};

    use super::forge_for;
    use crate::Remark;

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
        let quiet = |_: Remark| {};
        assert!(forge_for(&github_config(), &elsewhere, &quiet).is_none());
    }

    #[test]
    fn forge_for_builds_the_forge_the_named_token_reaches() {
        let bound = Bound(BTreeMap::from([("ACME_TOKEN", "t")]));
        let quiet = |_: Remark| {};
        assert!(forge_for(&github_config(), &bound, &quiet).is_some());
    }
}
