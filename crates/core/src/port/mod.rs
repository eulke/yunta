//! What the engine calls, and what answers.
//!
//! Two ports: [`session`], the agent CLI an adapter drives, and
//! [`forge`], the pull request an external gate is decided on. Both live
//! in this crate so the engine depends on their contracts rather than
//! on a concrete implementation.

pub mod forge;
pub mod policy;
pub mod session;

pub use forge::{
    Forge, ForgeError, PolledGate, PublishRequest, PublishedGate, ReviewComment, ReviewOutcome,
};
pub use policy::{absence_of, Absence, POLICY};
pub use session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, Budget, CodecError, FenceCodec,
    HookReply, PermissionProfile, ProbeReport, RunToolsEndpoint, SessionRequest,
};
