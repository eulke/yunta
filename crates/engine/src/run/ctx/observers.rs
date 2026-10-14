//! What a run's context answers the rest of the engine with: how a command's
//! output is kept, and how a task cycle's events are appended to the log.

use yunta_core::events::EventPayload;
use yunta_core::NodeId;
use yunta_storage::StorageError;

use super::RunCtx;

#[async_trait::async_trait]
impl crate::task_cycle::KeepsOutput for RunCtx<'_> {
    async fn keep_output(
        &self,
        output: &crate::process::CommandOutput,
    ) -> std::io::Result<yunta_core::ContentHash> {
        crate::artifacts::store::ObjectStore::at(self.run_dir)
            .redacting(&self.redactor)
            .keep_output(output)
            .await
    }

    async fn kept_output(&self, hash: &yunta_core::ContentHash) -> Option<Vec<u8>> {
        crate::artifacts::store::ObjectStore::at(self.run_dir)
            .redacting(&self.redactor)
            .kept_output(hash)
            .await
    }
}

/// `RunCtx` is the one real [`SessionObserver`] — audit events
/// land in the run's own log as they arrive, so a concurrent `status`
/// sees the live session, and they reach the invocation's display
/// surface on the same path every other event of the run takes. An
/// append that fails carries its storage cause back to the dispatch,
/// which fails the node with it.
#[async_trait::async_trait]
impl crate::task_cycle::SessionObserver for RunCtx<'_> {
    async fn record(
        &self,
        node_id: &NodeId,
        payload: EventPayload,
    ) -> Result<yunta_core::Seq, StorageError> {
        // An event of the cycle that cannot be appended is not
        // dropped: it would silently thin the trail `status` and replay
        // read (a lost `agent_session_opened` even changes what a resume
        // finds), so the storage cause travels back to the cycle and
        // fails the node — the same storage the run's next mandatory
        // event would hit anyway, surfaced now instead of masked.
        self.log().record(Some(node_id), payload).await
    }

    async fn refused_writes(
        &self,
        sessions: &[yunta_core::SessionId],
    ) -> Result<Vec<yunta_core::events::ToolTarget>, StorageError> {
        let events = self.log().events().await?;
        Ok(crate::run_tools::refused_writes(&events, sessions))
    }

    fn process_registry(&self) -> Option<&crate::process_registry::ProcessRegistry> {
        self.process_registry.as_deref()
    }

    async fn host_settled(
        &self,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<bool, StorageError> {
        self.awake.settled(&self.log(), cancel).await
    }

    fn awake(&self) -> std::time::Instant {
        self.clock.awake()
    }

    fn shell_service(&self) -> Option<&std::collections::BTreeMap<String, String>> {
        self.ambient.map(|ambient| &ambient.service)
    }
}
