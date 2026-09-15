//! What a test asks a [`Bench`] about: the directories its run owns,
//! the artifacts it holds, the events it wrote and the commits it left.
//!
//! Every answer is derived from the run's own log or from what the run
//! stored, never from a file that happens to sit beside it.

use std::sync::Arc;

use yunta_adapters::MockAdapter;
use yunta_core::events::StoredEvent;
use yunta_core::{ConfigLayer, Workflow};

use super::Bench;

impl Bench {
    /// The absolute run dir this bench's run uses — known before the run
    /// exists, so a fixture can embed the absolute artifact paths a real
    /// agent would write after reading `{{run.dir}}`.
    pub fn run_dir(&self) -> std::path::PathBuf {
        self.runs_root.join(self.run_id.as_str())
    }

    /// The directory one node writes the files it declares into — known
    /// before the run exists, so a fixture can embed the absolute paths a
    /// session is granted and writes to.
    pub fn staging(&self, node: &str) -> std::path::PathBuf {
        yunta_engine::run_dir::staging(&self.run_dir(), &node.into())
    }

    /// The bytes the run holds for one artifact, by the identity it is
    /// declared under: a kind name (`tasks`) for a document the engine
    /// reads, a file name for an opaque one.
    ///
    /// What a reader of the run gets: the acceptance standing last on the
    /// run's own log for that identity, read out of its object store. A
    /// test asserts on the run's answer rather than on a file that
    /// happens to sit beside it. `None` when the run's log holds no such
    /// artifact.
    pub fn artifact(&self, declared: &str) -> Option<Vec<u8>> {
        let spec: yunta_core::ArtifactSpec = yunta_core::yaml::parse(declared)
            .expect("an artifact is named the way `artifacts.produces` names one");
        let id = yunta_core::events::ArtifactId::from(&spec);
        let held = crate::events::accepted(&self.events());
        let found = held
            .iter()
            .filter(|a| a.artifact == id)
            .max_by_key(|a| a.seq)?;
        self.object(&found.content_hash).ok()
    }

    /// The bytes of one artifact's `artifacts/` view — a producer's under
    /// its node, what the run acquired without one at the root.
    ///
    /// For a test about the projection itself, or about what a session
    /// left in the run's `artifacts/` directory; every test about what
    /// the run holds asks [`artifact`](Self::artifact).
    pub fn projection(&self, producer: Option<&str>, name: &str) -> std::io::Result<Vec<u8>> {
        let mut path = self.run_dir().join(yunta_core::ARTIFACTS_DIR);
        if let Some(node) = producer {
            path.push(node);
        }
        std::fs::read(path.join(name))
    }

    /// The bytes the run stored under `hash` — what an acceptance names,
    /// read straight out of the object store.
    pub fn object(&self, hash: &yunta_core::ContentHash) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.run_dir().join("objects").join(hash.as_str()))
    }

    /// Every artifact this run's log says it accepted, in log order.
    pub fn accepted(&self) -> Vec<yunta_core::events::artifacts::ArtifactRef> {
        crate::events::accepted(&self.events())
    }

    /// The adapter the last run used — what a test asks about the
    /// requests the engine actually made.
    pub fn mock(&self) -> Arc<MockAdapter> {
        self.driven(|driven| driven.mock.clone())
    }

    /// Every event this bench's run has appended.
    pub fn events(&self) -> Vec<StoredEvent> {
        self.storage
            .events_for_run(&self.run_id)
            .expect("read events")
    }

    /// The manifest the run this bench created froze — what a receipt
    /// is read against.
    pub fn manifest(&self) -> yunta_core::Manifest {
        self.driven(|driven| driven.manifest.clone())
    }

    /// The manifest `(workflow, config)` freezes against this bench's
    /// worktree, with no run created — for a test about what building
    /// one produces.
    pub async fn manifest_for(
        &self,
        workflow_yaml: &str,
        config_yaml: &str,
    ) -> yunta_core::Manifest {
        let workflow: Workflow = serde_norway::from_str(workflow_yaml).expect("parse workflow");
        let config: ConfigLayer = serde_norway::from_str(config_yaml).expect("parse config");
        self.freeze(&workflow, &config).await.manifest
    }

    /// Every finding `node` posted on this run's log, in log order.
    pub fn findings_by(&self, node: &str) -> Vec<yunta_core::events::Finding> {
        self.events()
            .into_iter()
            .filter(|event| event.node_id.as_ref().is_some_and(|id| id.as_str() == node))
            .filter_map(|event| match event.payload() {
                Some(yunta_core::events::EventPayload::Findings(
                    yunta_core::events::FindingEvent::Posted(posted),
                )) => Some(posted.finding.clone()),
                _ => None,
            })
            .collect()
    }

    /// What one group of reviewers left on its blackboard — the
    /// node-output a `context: [{ node-output: { node: <group> } }]`
    /// source reads. Empty when the group wrote nothing.
    pub fn group_output(&self, group: &str) -> String {
        std::fs::read_to_string(
            self.run_dir()
                .join("node-output")
                .join(format!("{group}.txt")),
        )
        .unwrap_or_default()
    }

    /// The subjects of the commits this run left on the worktree's
    /// branch, oldest first — the seed commit every bench starts from
    /// excluded, so what a test reads is what its run committed.
    pub fn commit_subjects(&self) -> Vec<String> {
        crate::repo::git_output(&self.worktree, &["log", "--format=%s", "--reverse"])
            .lines()
            .filter(|subject| *subject != "initial")
            .map(str::to_string)
            .collect()
    }
}
