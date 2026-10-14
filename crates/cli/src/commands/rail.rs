//! `yunta rail <adapter-id> <event>` — the hook a CLI runs while it works.
//!
//! The agent's CLI executes this with what it is about to do on stdin and
//! where its session's engine listens in its environment. The adapter's
//! codec reads the call, the engine answers over that session's listener,
//! and the codec writes the answer back in the shape that CLI reads.
//!
//! A hook that cannot ask answers as one that could not: a write is
//! refused, never allowed on a guess — a hook that does not know what the
//! session may write letting the write through is the one outcome nobody
//! could defend.

use std::time::Instant;

use yunta_core::port::{HookReply, RailCodec};
use yunta_core::rail::answer::{FormatAnswer, HookAnswer, StopAnswer, WriteAnswer};
use yunta_core::rail::hook::{settle, RailFailure};
use yunta_core::rail::window::{AskBudget, Wait};
use yunta_core::rail::{RailEndpoint, RailEvent};

/// What the call fails with when the adapter has no codec to read it: a
/// refusal, with the reason on stderr where the CLI shows it.
const MISCONFIGURED: i32 = 2;

/// Answers one hook call at its moment and writes the answer the codec
/// reads. A write waits for the engine inside the window this process has
/// left, counted from `started`, and is refused once that window is spent.
/// An after-write waits the same way and is told what its formatters did.
/// A stop waits for the engine's judgement without a window of its own; a
/// stop that cannot be asked lets the session stop, and the attempt's close
/// still judges the work.
pub fn run(
    codec: Option<&dyn RailCodec>,
    event: RailEvent,
    endpoint: Option<&RailEndpoint>,
    stdin: &[u8],
    started: Instant,
) -> HookReply {
    let Some(codec) = codec else {
        return misconfigured("this adapter has no rail codec".to_string());
    };
    let call = codec
        .decode(event, stdin)
        .map_err(|source| RailFailure::Undecodable(yunta_core::describe(&source)));
    let endpoint = endpoint.ok_or(RailFailure::NoEngine);
    let window = Wait::Write(AskBudget::from_process_start(started));
    match event {
        RailEvent::BeforeWrite => settle::<WriteAnswer>(endpoint, call, window).reply(codec),
        RailEvent::AfterWrite => settle::<FormatAnswer>(endpoint, call, window).reply(codec),
        RailEvent::BeforeStop => settle::<StopAnswer>(endpoint, call, Wait::Judgement).reply(codec),
    }
}

fn misconfigured(reason: String) -> HookReply {
    HookReply {
        stdout: Vec::new(),
        stderr: format!("yunta: rail misconfigured: {reason}\n").into_bytes(),
        exit: MISCONFIGURED,
    }
}
