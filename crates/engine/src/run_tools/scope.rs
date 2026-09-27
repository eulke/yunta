//! The tools that speak about the scope a session works to: what of its
//! node's work lies outside it, and how the session asks for it to be
//! widened.
//!
//! A session never widens its own scope: the request is written as the
//! very file the file-based path produces, and the engine decides it when
//! the attempt ends — for a task by its loop's rules or a person, for a
//! node by a person, from the menu of the failure the request becomes.
//! One mechanism with two intake surfaces, rather than a second policy
//! living behind the listener.

use serde::Serialize;
use serde_json::Value;
use yunta_core::ScopeGlob;

use super::session::{RunToolError, SessionTools};
use super::tasks::render;

impl SessionTools {
    /// What this node's close would find outside its scope if the
    /// session ended now: the same audit, from the tree the attempt's
    /// start recorded, against the scope the close holds it to.
    pub(super) async fn check_scope(&self) -> Result<String, RunToolError> {
        let access = self
            .node_scope
            .as_deref()
            .ok_or(RunToolError::NoNodeScope)?;
        let state = crate::replay::derive(&self.events().await?);
        let from = state
            .nodes
            .from_tree(&self.node)
            .cloned()
            .ok_or(RunToolError::NoStartingTree)?;
        let result = crate::scope::audit(
            &self.cwd,
            &from,
            &access.index,
            &access.scope,
            access.staged.get().map_or(&[], Vec::as_slice),
            self.host.supervision(&self.stop),
        )
        .await
        .map_err(|source| RunToolError::Audit { source })?;
        render(&ScopeVerdict {
            within: result.violations.is_empty(),
            scope: &access.scope,
            outside_scope: result.violations,
        })
    }

    pub(super) async fn request_scope_expansion(
        &self,
        args: serde_json::Map<String, Value>,
    ) -> Result<String, RunToolError> {
        let answered_by_a_person = match (&self.task, &self.node_scope) {
            (Some(_), _) => false,
            (None, Some(access)) if access.may_ask => true,
            _ => return Err(RunToolError::NoScopeToWiden),
        };
        // The identical request object the file-based path uses,
        // validated by the same type it parses — then written as that
        // exact file, so what consumes it at the attempt's end reads it
        // unchanged: one mechanism, two intake surfaces.
        let request: crate::scope_expansion::ScopeExpansionRequest =
            serde_json::from_value(Value::Object(args))
                .map_err(|source| RunToolError::InvalidRequest { source })?;
        let path = self
            .cwd
            .join(crate::scope_expansion::SCOPE_EXPANSION_REQUEST_FILE);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Err(RunToolError::RequestPending);
        }
        let yaml = yunta_core::yaml::to_string(&request)
            .map_err(|source| RunToolError::Yaml { source })?;
        tokio::fs::write(&path, yaml)
            .await
            .map_err(|source| RunToolError::Write {
                path: path.clone(),
                source,
            })?;
        Ok(if answered_by_a_person {
            "request recorded — when this attempt ends a person decides whether to widen \
             this node's scope; finish what you can within it and do not write the paths \
             you asked for"
        } else {
            "request recorded — it is evaluated when this attempt ends (the engine \
             or a person decides; a denial becomes a finding); re-attempt the work after"
        }
        .to_string())
    }
}

/// A node's work against its scope, as `yunta_check_scope` answers it.
#[derive(Serialize)]
struct ScopeVerdict<'a> {
    /// True when nothing the node changed lies outside its scope.
    within: bool,
    /// What the node may write: what it declared plus what was granted.
    scope: &'a [ScopeGlob],
    /// What it changed outside that scope.
    outside_scope: Vec<std::path::PathBuf>,
}
