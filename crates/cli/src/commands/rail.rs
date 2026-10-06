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

use std::time::Duration;

use yunta_core::port::{HookReply, RailCodec};
use yunta_core::rail::{unasked, RailCall, RailEndpoint, RailEvent};

/// How long a write hook waits for the engine before it answers on its
/// own — well inside the time a CLI gives that hook, so the CLI never runs
/// out of patience first and lets a write through unanswered. A stop hook
/// waits as long as the engine judges: a CLI that runs out of patience
/// there lets the session end, which the close still judges.
const DEADLINE: Duration = Duration::from_secs(8);

/// What the call fails with when it cannot be read or asked at all:
/// a refusal, with the reason on stderr where the CLI shows it.
const MISCONFIGURED: i32 = 2;

pub fn run(
    codec: Option<&dyn RailCodec>,
    event: RailEvent,
    endpoint: Option<&RailEndpoint>,
    stdin: &[u8],
    ask: impl FnOnce(&RailEndpoint, &RailCall) -> Result<yunta_core::rail::RailAnswer, String>,
) -> HookReply {
    let Some(codec) = codec else {
        return misconfigured("this adapter has no rail codec".to_string());
    };
    let call = match codec.decode(event, stdin) {
        Ok(Some(call)) => call,
        // A call with nothing to ask goes on without asking.
        Ok(None) => return codec.encode(event, &yunta_core::rail::RailAnswer::Proceed),
        Err(source) => {
            return codec.encode(event, &unasked(event, &yunta_core::describe(&source)));
        }
    };
    let Some(endpoint) = endpoint else {
        return codec.encode(
            event,
            &unasked(event, "this session's environment names no engine to ask"),
        );
    };
    let answer = ask(endpoint, &call).unwrap_or_else(|why| unasked(event, &why));
    codec.encode(event, &answer)
}

/// The engine's answer, asked over the loopback listener.
pub fn over_loopback(
    endpoint: &RailEndpoint,
    call: &RailCall,
) -> Result<yunta_core::rail::RailAnswer, String> {
    let deadline = match call.act.moment() {
        RailEvent::BeforeWrite | RailEvent::AfterWrite => Some(DEADLINE),
        RailEvent::BeforeStop => None,
    };
    endpoint
        .ask(call, deadline)
        .map_err(|source| yunta_core::describe(&source))
}

fn misconfigured(reason: String) -> HookReply {
    HookReply {
        stdout: Vec::new(),
        stderr: format!("yunta: rail misconfigured: {reason}\n").into_bytes(),
        exit: MISCONFIGURED,
    }
}
