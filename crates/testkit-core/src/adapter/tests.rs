//! The helpers' own promises.

use std::sync::mpsc;
use std::time::Duration;

use super::{child_pid_fifo, grandchild_pid_within};

/// A stub that never records its child's pid fails the test waiting for it
/// instead of hanging it: a runtime waits for its blocking tasks before it
/// ends, so the reader blocked on the fifo must be released.
#[test]
fn a_pid_never_recorded_fails_the_wait_instead_of_hanging_it() {
    let dir = std::env::temp_dir().join(format!("yunta-fifo-wait-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = child_pid_fifo(&dir);
    let (sent, ended) = mpsc::channel();
    let waiting = fifo.clone();
    std::thread::spawn(move || {
        let waited = std::panic::catch_unwind(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap();
            runtime.block_on(grandchild_pid_within(&waiting, Duration::from_millis(50)))
        });
        sent.send(waited.is_err()).unwrap();
    });

    let failed = ended.recv_timeout(Duration::from_secs(10));

    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(failed, Ok(true), "the wait fails instead of hanging");
}
