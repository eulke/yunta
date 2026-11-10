//! See [`super`]. What the project's config says of packs: the publishers it
//! allows, and what it does with a pack that ships executors, merged across
//! its layers with each restriction kept beside the layers that declare it. A
//! refusal that cannot say which config file to change is not one a person
//! can act on: a ceiling a higher layer sets may name a file they cannot
//! even edit.

use std::path::Path;

use yunta_core::refusal::{PackRefusal, PackVerb};
use yunta_core::{ConfigLayer, Env, PackExecutorPolicy, PackManifest, Publisher};

use super::refused;
use crate::Error;

/// The merged `permissions.packs`, with the layers that declare each
/// restriction named.
pub(super) struct PackPolicy {
    /// A non-empty allow-list, with the layers that declare it; `None` when
    /// no layer restricts publishers, which allows every one.
    publishers_allow: Option<(Vec<Publisher>, Vec<&'static str>)>,
    /// The merged executor policy, the strictest winning; `Prompt` when no
    /// layer declares one.
    executors: PackExecutorPolicy,
    /// The layers declaring the executor policy that won the merge.
    executors_declared_by: Vec<&'static str>,
}

impl PackPolicy {
    /// The policy the config layers of the project at `dir` set, read with
    /// `env`.
    pub(super) fn of(dir: &Path, env: &Env) -> Result<Self, Error> {
        let layers = crate::config_layers(dir, env)?;
        let merged = ConfigLayer::merge_layers(layers.iter().map(|(_, layer)| layer.clone()));
        let packs = merged
            .permissions
            .as_ref()
            .and_then(|permissions| permissions.packs.as_ref());
        let declaring = |declares: &dyn Fn(&ConfigLayer) -> bool| -> Vec<&'static str> {
            layers
                .iter()
                .filter(|(_, layer)| declares(layer))
                .map(|(name, _)| *name)
                .collect()
        };
        let allow = packs
            .and_then(|packs| packs.publishers.as_ref())
            .map(|publishers| publishers.allow.clone())
            .filter(|allow| !allow.is_empty());
        let publishers_allow = allow.map(|allow| {
            let declared_by = declaring(&|layer| {
                layer
                    .permissions
                    .as_ref()
                    .and_then(|permissions| permissions.packs.as_ref())
                    .and_then(|packs| packs.publishers.as_ref())
                    .is_some_and(|publishers| !publishers.allow.is_empty())
            });
            (allow, declared_by)
        });
        let executors = packs
            .and_then(|packs| packs.executors)
            .unwrap_or(PackExecutorPolicy::Prompt);
        let executors_declared_by = declaring(&|layer| {
            layer
                .permissions
                .as_ref()
                .and_then(|permissions| permissions.packs.as_ref())
                .and_then(|packs| packs.executors)
                == Some(executors)
        });
        Ok(PackPolicy {
            publishers_allow,
            executors,
            executors_declared_by,
        })
    }

    /// Whether the executors `manifest` declares wait on a person's
    /// confirmation before the pack is vendored: under a policy that asks,
    /// until they confirm.
    pub(super) fn waits_on_yes(&self, manifest: &PackManifest, confirmed: bool) -> bool {
        !manifest.declares.executors.is_empty()
            && self.executors == PackExecutorPolicy::Prompt
            && !confirmed
    }

    /// Refuses a publisher outside a non-empty allow-list, naming the layers
    /// that declare it.
    pub(super) fn admit_publisher(&self, publisher: &Publisher) -> Result<(), Error> {
        let Some((allowed, declared_by)) = &self.publishers_allow else {
            return Ok(());
        };
        if allowed.contains(publisher) {
            return Ok(());
        }
        Err(refused(PackRefusal::PublisherNotAllowed {
            publisher: publisher.clone(),
            allowed: allowed.clone(),
            declared_by: declared_by.clone(),
        }))
    }

    /// Refuses, before anything is shown, a pack whose executors the policy
    /// denies: no person's decision is left to make for it.
    pub(super) fn refuse_denied(
        &self,
        manifest: &PackManifest,
        verb: PackVerb,
    ) -> Result<(), Error> {
        if self.executors == PackExecutorPolicy::Deny {
            return self.admit_executors(manifest, false, verb);
        }
        Ok(())
    }

    /// The executor policy on what `manifest` declares, consulted only when
    /// it declares executors: `deny` refuses whatever was confirmed, a
    /// ceiling no flag overrides; `prompt` waits on a person's confirmation;
    /// `allow` passes.
    pub(super) fn admit_executors(
        &self,
        manifest: &PackManifest,
        confirmed: bool,
        verb: PackVerb,
    ) -> Result<(), Error> {
        let executors = manifest.declares.executors.len();
        if executors == 0 {
            return Ok(());
        }
        match self.executors {
            PackExecutorPolicy::Allow => Ok(()),
            PackExecutorPolicy::Deny => Err(refused(PackRefusal::ExecutorsDenied {
                executors,
                declared_by: self.executors_declared_by.clone(),
                verb,
            })),
            PackExecutorPolicy::Prompt if confirmed => Ok(()),
            PackExecutorPolicy::Prompt => Err(refused(PackRefusal::ExecutorsUnconfirmed {
                executors,
                verb,
            })),
        }
    }
}

/// Refuses a pack whose `yunta_schema` range leaves out the schema this
/// binary speaks: no run could use its workflows, and a tree vendored for
/// nothing is a failure found later with a tree to clean up.
pub(super) fn admit_schema(manifest: &PackManifest) -> Result<(), Error> {
    let Some(range) = &manifest.yunta_schema else {
        return Ok(());
    };
    if range.holds_for(yunta_core::YUNTA_SCHEMA) {
        return Ok(());
    }
    Err(refused(PackRefusal::SchemaOutside {
        pack: manifest.reference(),
        range: range.clone(),
        speaks: yunta_core::YUNTA_SCHEMA,
    }))
}
