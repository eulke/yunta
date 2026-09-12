//! The per-run MCP server — one loopback HTTP
//! listener **per node session**, never per run: it is born just before
//! the session spawns, dies with it, and a resume always mints a fresh
//! listener and credential (a credential that survives its session is
//! reuse surface). The engine is the server, the
//! adapter translates the endpoint to its CLI's native external-MCP
//! mechanism, the agent is the client.
//!
//! **The data never lives in the listener.** Every tool reads or writes
//! the run's own storage — which is why a blackboard stays readable
//! after the join even though the listeners that posted to it are gone,
//! and why nothing here needs recovering after a crash: the listener
//! dies with the `yunta run` process it lives inside.
//!
//! **Scoping by construction.** The bearer token authenticates
//! exactly one session; the listener itself holds that session's
//! `(run_id, node_id, task)` and no tool takes a run id as a caller
//! argument — a "read me some other run" call has no surface to exist
//! on. The read restriction on the blackboard is structural too:
//! `yunta_get_blackboard` only ever serves the calling node's own posts
//! — a sibling's posts become readable only through the group's
//! post-join consolidation, never through this listener.
//!
//! **Shell edge, deliberately.** This module is the imperative shell's
//! outermost boundary — a network listener serving a live agent. The
//! high entropy the token is generated with (uuid v4) lives here and
//! only here; it participates in no pure derivation. The events a tool
//! writes, though, are the run's events like any other: they carry the
//! run's own injected [`Clock`](yunta_core::Clock), shared with every
//! other emitter, so a run driven by a fixed clock produces reproducible
//! timestamps throughout — the host holds an `Arc<dyn Clock>` for
//! exactly that.
//!
//! **Where each part lives.** The chain runs outside in: [`host`] holds
//! what every listener of one run shares, [`listener`] is the socket and
//! the credential that gate it, [`session`] is the tool surface bound to
//! one session and the dispatch that routes a call, and [`catalog`] is
//! what that surface advertises. Each tool family then answers for
//! itself — [`submission`] for documents, [`findings`] for findings,
//! [`blackboard`] for the group's shared view, [`tasks`] for the tasks document —
//! and the two text modules hold every sentence a session reads:
//! [`notice`] before it calls anything, [`verdicts`] in answer to a call.

mod blackboard;
mod catalog;
mod findings;
mod host;
mod listener;
mod notice;
mod session;
mod submission;
mod tasks;
mod verdicts;

pub use blackboard::consolidate_blackboard;
pub use host::{RunToolsAccess, RunToolsHost};
pub use listener::{open_session_listener, RunToolsSession};
pub(crate) use notice::submission_notice;
