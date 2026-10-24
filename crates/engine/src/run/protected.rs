//! What no session of a run may write, as one wake of the run reads it:
//! the project's deny and the tests of the spec the run accepted last,
//! which a person approved, besides what every run protects and what the
//! links its base commit holds where an agent CLI loads resolve to.
//!
//! A wake reads the set once, from the events it already loads. It reads
//! again only when it accepts a spec, the one event that changes the set.
//! Every context of the run shares the set the last read stored, so a
//! fence, a close or a scope decision reads the same answer.

use std::path::Path;
use std::sync::Arc;

use yunta_core::events::artifacts::{ArtifactLedger, ArtifactRef};
use yunta_core::events::StoredEvent;
use yunta_core::protected::Protected;
use yunta_core::{ArtifactKind, Manifest, ScopeGlob, SpecFile};

use super::{RunCtx, RunError};
use crate::artifacts::store::ObjectStore;
use crate::replay::RunState;

/// What no session of the run may write, as its log stands: the project's
/// deny, and the tests of the spec the log accepted last.
pub(crate) async fn of(ctx: &RunCtx<'_>, events: &[StoredEvent]) -> Result<Protected, RunError> {
    let approved = match crate::artifacts::latest::<SpecFile>(ctx.run_dir, events).await? {
        Some(held) => held.document.test_globs(),
        None => Vec::new(),
    };
    build(ctx.manifest, &approved)
}

/// The set [`of`] builds, read from a run's state alone: the menu a person
/// answers is built from it, so it offers no grant a wake would refuse.
pub(crate) async fn of_state(
    manifest: &Manifest,
    run_dir: &Path,
    state: &RunState,
) -> Result<Protected, RunError> {
    let approved = match accepted_spec(&state.artifacts) {
        Some(held) => {
            let bytes = ObjectStore::at(run_dir).get(&held.content_hash).await?;
            yunta_core::shape::read::<SpecFile>(&bytes, crate::artifacts::describe(held))?
                .test_globs()
        }
        None => Vec::new(),
    };
    build(manifest, &approved)
}

/// The spec the log accepted last: the one whose tests are approved.
pub(crate) fn accepted_spec(artifacts: &ArtifactLedger) -> Option<&ArtifactRef> {
    artifacts.of_kind(ArtifactKind::Spec).last()
}

/// Compiles the parts into one set, grown by the links the run's base commit
/// holds where an agent CLI loads its configuration or its instructions. A
/// set that does not compile is the run's diagnostic: no decision is taken
/// from a partial set.
fn build(manifest: &Manifest, approved: &[ScopeGlob]) -> Result<Protected, RunError> {
    let links = &manifest.loaded_links;
    Protected::of_run(manifest.config.denied_paths(), approved, links).map_err(|error| {
        RunError::Broken {
            diagnostic: error.to_string(),
        }
    })
}

impl RunCtx<'_> {
    /// The set no session of this run may write, as the last read stored it.
    pub(crate) fn protected(&self) -> Arc<Protected> {
        let current = self
            .protected
            .read()
            .unwrap_or_else(|error| error.into_inner());
        Arc::clone(&*current)
    }

    /// Stores `protected` as the set every context of the run reads, and
    /// hands its pathspecs to the memo and to the checkout pool: a capture
    /// of a tree takes in the ignored paths they name, and a checkout given
    /// back is cleaned of them.
    pub(crate) fn renew_protected(&self, protected: Protected) {
        let pathspecs = protected.pathspecs();
        *self
            .protected
            .write()
            .unwrap_or_else(|error| error.into_inner()) = Arc::new(protected);
        self.memo.protect(pathspecs.clone());
        self.pool.protect(pathspecs);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use yunta_core::protected::Protection;

    use super::*;

    fn manifest() -> Manifest {
        let workflow = yunta_core::yaml::parse(
            "name: w\nnodes:\n  - { id: review, kind: bash, run: 'true' }\n",
        )
        .expect("a one-node workflow");
        Manifest {
            schema_version: <Manifest as yunta_core::persisted::Persisted>::SCHEMA_VERSION,
            yunta_version: "0.0.5".to_string(),
            workflow,
            config: yunta_core::ConfigLayer::default(),
            inputs: std::collections::BTreeMap::new(),
            prompts: std::collections::BTreeMap::new(),
            base_branch: "main".to_string(),
            base_commit: "deadbeef".into(),
            isolation: yunta_core::Isolation::None,
            max_parallel_nodes: 1,
            workflow_hash: yunta_core::sha256_hex(b"workflow"),
            config_hash: yunta_core::sha256_hex(b"config"),
            paths: None,
            pack: None,
            project: None,
            adapter: None,
            frozen: None,
            provenance_gaps: Vec::new(),
            loaded_links: Vec::new(),
        }
    }

    /// A run's pool cleans what the run protects from its first take, before
    /// any wake renews the set: the project's deny and what every run protects.
    #[test]
    fn a_run_s_pool_protects_from_its_first_take() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = manifest();
        manifest.config =
            yunta_core::yaml::parse("permissions:\n  paths:\n    deny: [\".github/**\"]\n")
                .expect("a config that denies one tree");
        let clock: Arc<dyn yunta_core::Clock> = Arc::new(yunta_testkit_core::FixedClock);
        let pool = crate::worktree::CheckoutPool::of_run(
            dir.path(),
            dir.path(),
            dir.path(),
            &manifest,
            clock,
        );
        let covered = pool.pathspecs();
        assert!(
            covered.rooted.contains(&":(glob).github/**".to_string()),
            "{covered:?}"
        );
        assert!(
            covered.rooted.contains(&":(glob).claude/**".to_string()),
            "{covered:?}"
        );
        assert!(
            covered.loose.contains(&":(glob)**/AGENTS.md".to_string()),
            "{covered:?}"
        );
    }

    /// What a committed link at a CLI's configuration resolves to is
    /// protected for that reason by every read of the set, and the run's
    /// pool cleans it from its first take.
    #[tokio::test]
    async fn a_link_a_cli_loads_through_protects_its_target_in_the_set_and_the_pool() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = manifest();
        manifest.loaded_links = vec![yunta_core::protected::LoadedLink {
            path: ".claude/hooks".into(),
            target: "tools/hooks".into(),
            passed: vec!["tools".into()],
            tree: true,
        }];
        let set = of_state(&manifest, dir.path(), &RunState::default())
            .await
            .unwrap();
        assert_eq!(
            set.reason_of(Path::new("tools/hooks/run.sh")),
            Some(Protection::AgentConfig)
        );
        let clock: Arc<dyn yunta_core::Clock> = Arc::new(yunta_testkit_core::FixedClock);
        let pool = crate::worktree::CheckoutPool::of_run(
            dir.path(),
            dir.path(),
            dir.path(),
            &manifest,
            clock,
        );
        let covered = pool.pathspecs();
        assert!(
            covered
                .rooted
                .contains(&":(glob)tools/hooks/**".to_string()),
            "{covered:?}"
        );
    }

    /// Every agent CLI's configuration is protected, whichever adapters the
    /// run drives: a state names none, and the set does not ask.
    #[tokio::test]
    async fn a_state_protects_the_configuration_of_every_agent_cli() {
        let dir = tempfile::tempdir().unwrap();
        let set = of_state(&manifest(), dir.path(), &RunState::default())
            .await
            .unwrap();
        assert_eq!(
            set.reason_of(Path::new(".claude/settings.json")),
            Some(Protection::AgentConfig)
        );
        assert_eq!(
            set.reason_of(Path::new(".codex/state/cache.json")),
            Some(Protection::AgentConfig)
        );
        assert_eq!(
            set.reason_of(Path::new(".agents/skills/grill/SKILL.md")),
            Some(Protection::AgentConfig)
        );
        assert_eq!(set.reason_of(Path::new(".agents/notes.md")), None);
    }
}
