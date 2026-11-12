//! What the engine calls, and what answers: the port an adapter implements
//! for its CLI, the one a forge implements for its pull requests, and the one
//! a surface implements for the person at it and for the run it draws.
//!
//! Four ports: [`session`], the agent CLI an adapter drives;
//! [`forge`], the pull request an external gate is decided on;
//! [`HumanInteraction`], the person a gate's decision or a node's questions
//! are put to, which a surface's console answers; and
//! [`RunObserver`], whoever draws a run as the engine writes its log and
//! hears what its sessions say. [`policy`]
//! says what the engine does when an adapter lacks a capability, and
//! [`SessionRequest::redactor`] takes every secret a session holds out of
//! what it says. The
//! ports live in this crate so the engine depends on their contracts
//! rather than on a concrete implementation, and the engine's library
//! never links the crate the implementations are built in.
//!
//! A port is a trait the engine calls, never a network port: the loopback
//! port a session's run tools listen on is named by a [`RunToolsEndpoint`],
//! which `session_env` holds beside the environment a session's CLI runs in.

pub mod forge;
mod observer;
mod person;
pub mod policy;
mod secrets;
pub mod session;
mod session_env;

pub use forge::{
    Forge, ForgeError, ForgeProbe, GateDecision, PolledGate, PublishRequest, PullRequestRef,
    PullRequestRequest, ReviewComment, ReviewOutcome,
};
pub use observer::{Observed, RunObserver, Said};
pub use person::{DecisionAsked, HumanInteraction, NoInteraction, QuestionsReply};
pub use policy::{absence_of, Absence, Policy, PolicyApplied, POLICY};
pub use session::{
    Adapter, AgentError, AgentEvent, AgentOutcome, AgentSession, Budget, CodecError, HookReply,
    PermissionProfile, ProbeReport, RailCodec, RunToolsEndpoint, SessionRequest,
};
