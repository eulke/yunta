//! The two tools that speak about the run's tasks document: where its tasks
//! stand, and how a session asks for one task's scope to be widened.
//!
//! Both are read or ask, never write. A session never widens its own
//! scope: the request is written as the very file the file-based path
//! produces, so the engine's existing post-attempt evaluation
//! (rules/ask/deny, cap, findings on denial) decides it — one mechanism
//! with two intake surfaces, rather than a second policy living behind
//! the listener.

use serde_json::Value;

use super::session::{RunToolError, SessionTools};

impl SessionTools {
    pub(super) async fn task_status(&self) -> Result<String, RunToolError> {
        let state = crate::replay::derive(&self.events().await?);
        let mut tasks: Vec<(String, String)> = state
            .tasks
            .iter()
            .map(|(id, status)| {
                (
                    id.to_string(),
                    serde_json::to_value(status)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("{status:?}")),
                )
            })
            .collect();
        tasks.sort();
        let map: serde_json::Map<String, Value> = tasks
            .into_iter()
            .map(|(id, status)| (id, Value::String(status)))
            .collect();
        serde_json::to_string_pretty(&Value::Object(map))
            .map_err(|source| RunToolError::Render { source })
    }

    pub(super) fn request_scope_expansion(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        if self.task.is_none() {
            return Err(RunToolError::NoTask);
        }
        // The identical request object the file-based path uses,
        // validated by the same
        // type it parses — then written as that exact
        // file, so the engine's existing post-attempt evaluation
        // (rules/ask/deny, cap, findings on denial) consumes it
        // unchanged: one mechanism, two intake surfaces.
        let request: crate::scope_expansion::ScopeExpansionRequest =
            serde_json::from_value(Value::Object(args))
                .map_err(|source| RunToolError::InvalidRequest { source })?;
        let path = self
            .cwd
            .join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
        if path.exists() {
            return Err(RunToolError::RequestPending);
        }
        let yaml = yunta_core::yaml::to_string(&request)
            .map_err(|source| RunToolError::Yaml { source })?;
        std::fs::write(&path, yaml).map_err(|source| RunToolError::Write {
            path: path.clone(),
            source,
        })?;
        Ok(
            "request recorded — it is evaluated when this attempt ends (the engine \
             or a person decides; a denial becomes a finding); re-attempt the work after"
                .to_string(),
        )
    }
}
