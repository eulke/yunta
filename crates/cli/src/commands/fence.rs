//! `yunta fence <adapter-id>` — the hook a CLI runs before it writes.
//!
//! The agent's CLI executes this with the call it is about to make on
//! stdin and `YUNTA_FENCE` in its environment. The adapter's codec
//! reads the call, [`Fence::judge`] answers, and the codec writes the
//! answer back in the shape that CLI reads.
//!
//! A failure of ours refuses; it never allows. A hook that cannot read
//! its own configuration has no idea what the session may write, and
//! letting the write through on that basis is the one outcome nobody
//! could defend.

use yunta_core::fence::Fence;
use yunta_core::port::{FenceCodec, HookReply};

/// What a misconfiguration answers: refused, with the reason on stderr
/// where the CLI shows it.
const MISCONFIGURED: i32 = 2;

pub fn run(codec: Option<&dyn FenceCodec>, fence_var: Option<&str>, stdin: &[u8]) -> HookReply {
    let Some(codec) = codec else {
        return misconfigured("this adapter has no fence codec".to_string());
    };
    let Some(value) = fence_var else {
        return misconfigured(format!(
            "`{}` is not set in this session's environment",
            yunta_core::fence::ENV_VAR
        ));
    };
    let (fence, worktree) = match Fence::from_env(value) {
        Ok(read) => read,
        Err(source) => return misconfigured(yunta_core::describe(&source)),
    };
    let target = match codec.decode(stdin) {
        Ok(Some(target)) => target,
        // A call that writes no path writes nothing to judge.
        Ok(None) => return codec.encode(&yunta_core::fence::Verdict::Allowed),
        Err(source) => return misconfigured(yunta_core::describe(&source)),
    };
    codec.encode(&fence.judge(&worktree, &target))
}

fn misconfigured(reason: String) -> HookReply {
    HookReply {
        stdout: Vec::new(),
        stderr: format!("yunta: fence misconfigured: {reason}\n").into_bytes(),
        exit: MISCONFIGURED,
    }
}
