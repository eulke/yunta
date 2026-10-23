//! What the engine calls, and what answers: the port an adapter implements
//! for its CLI, and the one a forge implements for its pull requests.
//!
//! Two ports: [`session`], the agent CLI an adapter drives, and
//! [`forge`], the pull request an external gate is decided on. Both live
//! in this crate so the engine depends on their contracts rather than
//! on a concrete implementation.

pub mod forge;
pub mod policy;
pub mod session;

pub use forge::{
    Forge, ForgeError, ForgeProbe, GateDecision, PolledGate, PublishRequest, PullRequestRef,
    PullRequestRequest, ReviewComment, ReviewOutcome,
};
pub use policy::{absence_of, Absence, Policy, PolicyApplied, POLICY};
pub use session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, Budget, CodecError, HookReply,
    PermissionProfile, ProbeReport, RailCodec, RunToolsEndpoint, SessionRequest,
};
