//! A thread with room for the deepest run a test composes.

use std::future::Future;

/// The stack a deeply composed run needs, fixed by D170. A debug
/// build's futures are several times the size of a release build's, and
/// the margin is free: a thread's stack is reserved address space,
/// committed page by page as it is used.
const DEEP_STACK: usize = 64 * 1024 * 1024;

/// Runs `body` to completion on a thread with a stack deep enough for a
/// run that composes workflows more than two levels down.
///
/// A `kind: workflow` node drives its child's whole state machine
/// inside its own, so each level of composition nests one engine poll in
/// another; in a debug build three levels outgrow the 8 MiB a test
/// thread gets.
/// A panic inside `body` — an assertion, above all — reaches the test
/// harness unchanged.
///
/// Use it in place of `#[tokio::test]`:
///
/// ```ignore
/// #[test]
/// fn a_grandchild_sees_what_the_root_measured() {
///     on_a_deep_stack(|| async { /* … */ });
/// }
/// ```
pub fn on_a_deep_stack<F, Fut>(body: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()>,
{
    let thread = std::thread::Builder::new()
        .stack_size(DEEP_STACK)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build a current-thread runtime")
                .block_on(body());
        })
        .expect("spawn a thread with a deep stack");
    if let Err(panic) = thread.join() {
        std::panic::resume_unwind(panic);
    }
}
