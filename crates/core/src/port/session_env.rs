//! What a session's CLI is handed beside its prompt: the environment it runs
//! in, and where its run's tools listen. Both travel in the session's request
//! ([`SessionRequest`](super::SessionRequest)), and `session` re-exports them
//! where they have always been read.

use std::collections::BTreeMap;
use std::time::Duration;

use super::session::Budget;
use crate::Secret;

/// A session's environment, in two maps by what a log may carry: `plain`
/// for shared directories and engine settings, `secret` for declared
/// secrets and session tokens, which are never logged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionEnv {
    /// Shared directories and engine settings: not secrets.
    pub plain: BTreeMap<String, String>,
    /// Declared secrets and session tokens: never logged.
    pub secret: BTreeMap<String, Secret<String>>,
}

/// Where a session's per-run MCP server listens: a loopback URL plus
/// the per-session bearer token that scopes every call to `(run_id,
/// node_id, attempt)` by construction — no tool ever takes a run id as
/// a caller argument. The token is secret material: it must never
/// reach the event log — the engine's own audit events carry the URL
/// at most, never this pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunToolsEndpoint {
    pub url: String,
    pub token: Secret<String>,
}

impl RunToolsEndpoint {
    /// The file a session's adapter writes its tool endpoint to; it carries
    /// the session's bearer.
    pub const CONFIG_FILE: &'static str = "mcp.json";

    /// The name a CLI's own configuration gives this server. Every
    /// adapter uses the one name: a CLI prefixes the tools it mounts
    /// with it, so this is also what an allow-rule names to admit all
    /// of them without any adapter knowing which tools the engine
    /// mounted.
    ///
    /// It is not `yunta`, which is what a person registering the
    /// control plane in their CLI's own configuration calls it. A CLI
    /// merges both entries into one table by key, so two servers under
    /// one name is one server configured twice — and the second write
    /// contradicts the first, because the control plane is a command
    /// and this is a URL.
    pub const SERVER_NAME: &'static str = "yunta-run";

    /// How long one call to these tools may take before the CLI gives up
    /// on it: as long as the session itself may run, or an hour for a
    /// session with no timeout. A call can run a task's criteria,
    /// which take as long as the build behind them, and a CLI's own
    /// default would cut the answer off before it arrives.
    pub fn call_timeout(budget: &Budget) -> Duration {
        budget
            .timeout
            .unwrap_or(Duration::from_secs(crate::rail::window::UNSET_WINDOW_SECS))
    }
}
