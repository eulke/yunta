//! What every MCP result this workspace serves is shaped like.
//!
//! Two servers answer MCP here: the per-run tool listener the engine
//! starts for one session, and the control plane `yunta mcp` serves over
//! stdio. Both announce the same set of protocol revisions, so both owe
//! their clients the same result shape, and the decision lives above
//! both rather than once per server — a second copy is a second chance
//! for one of them to drift out of the era its own clients speak. It
//! lives in this crate because this is the lowest one that already
//! serves MCP, and the CLI depends on it.

use rmcp::model::{CacheScope, ListToolsResult, Tool};

/// `tools`, as a list result every client this server admits to serving
/// can read.
///
/// Revision `2026-07-28` makes the cache hints mandatory on every list
/// result: a client of that era validates the shape and drops the whole
/// list when one is missing, which leaves the session holding no tool at
/// all. A client of an earlier revision ignores a field it does not
/// know, so this one shape answers both eras and neither server has to
/// ask which one it is talking to.
///
/// `ttl_ms: 0` says the list is stale the moment it is read: it is built
/// per session and per node, so a client that caches must re-ask rather
/// than reuse another session's tools. `private` because the list names
/// what one authenticated caller may do, and a shared cache must never
/// hand it to a different one.
pub fn tool_list(tools: Vec<Tool>) -> ListToolsResult {
    ListToolsResult::with_all_items(tools)
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
}
