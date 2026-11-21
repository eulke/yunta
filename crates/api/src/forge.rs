//! See [`crate`]. The forge a run under a config reaches: what a run
//! publishes a gate and opens a pull request through, what `doctor` probes,
//! and what a run's screen polls a gate's pull request on.
//!
//! A workspace reaches it through the [`ForgeSource`] it is handed: every
//! interface hands [`DeclaredForge`], which builds the forge the config
//! declares with the token the workspace's secrets hold; a test hands one
//! that stands in for it. Why none answers is said once, as a
//! [`ForgeUnanswered`], for each caller to act on: a run's gate falls back to
//! the person, doctor reports it, a run's screen says it.

use std::sync::Arc;

use yunta_core::port::{Forge, ForgeUnanswered};
use yunta_core::{ConfigLayer, SecretSource};

/// Where the forge a run's config declares is reached from, with the secrets
/// its token is read from. Not a [`Forge`], one forge reached: this hands one
/// out for a config, or says why it reaches none.
pub trait ForgeSource: Send + Sync {
    /// The forge `config` declares, reached with the token `secrets` hold.
    fn forge(
        &self,
        config: &ConfigLayer,
        secrets: &dyn SecretSource,
    ) -> Result<Arc<dyn Forge>, ForgeUnanswered>;
}

/// The forge a config declares, built with the token the secrets hold: what
/// every interface reaches a forge through. It reaches none where the config
/// declares none, or the variable its token is in is not one the secrets hold
/// (a gate then falls back to the person, a legitimate state on a machine with
/// no credentials), nor where the forge cannot be made ready to be asked.
pub struct DeclaredForge;

impl ForgeSource for DeclaredForge {
    fn forge(
        &self,
        config: &ConfigLayer,
        secrets: &dyn SecretSource,
    ) -> Result<Arc<dyn Forge>, ForgeUnanswered> {
        let declared = config
            .forge
            .as_ref()
            .and_then(|forge| forge.github.as_ref());
        let github = declared.ok_or(ForgeUnanswered::Undeclared)?;
        let token = secrets
            .get(&github.token_env)
            .ok_or_else(|| ForgeUnanswered::TokenUnset {
                token_env: github.token_env.clone(),
            })?;
        match yunta_adapters::GitHubForge::new(github.repo.clone(), token) {
            Ok(forge) => Ok(Arc::new(forge)),
            Err(error) => Err(ForgeUnanswered::Failed {
                said: yunta_core::describe(&error),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yunta_core::port::ForgeUnanswered;
    use yunta_core::{ConfigLayer, Secret, SecretSource};

    use super::{DeclaredForge, ForgeSource};

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
    fn a_declared_forge_is_unreached_without_the_token_it_names() {
        let elsewhere = Bound(BTreeMap::from([("OTHER_TOKEN", "t")]));
        let unreached = DeclaredForge.forge(&github_config(), &elsewhere).err();
        let unset = ForgeUnanswered::TokenUnset {
            token_env: "ACME_TOKEN".to_string(),
        };
        assert_eq!(unreached, Some(unset));
    }

    #[test]
    fn a_declared_forge_is_reached_with_the_token_it_names() {
        let bound = Bound(BTreeMap::from([("ACME_TOKEN", "t")]));
        assert!(DeclaredForge.forge(&github_config(), &bound).is_ok());
    }

    #[test]
    fn a_config_that_declares_no_forge_reaches_none() {
        let bound = Bound(BTreeMap::from([("ACME_TOKEN", "t")]));
        let unreached = DeclaredForge.forge(&ConfigLayer::default(), &bound).err();
        assert_eq!(unreached, Some(ForgeUnanswered::Undeclared));
    }
}
