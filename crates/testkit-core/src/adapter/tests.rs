//! The helpers' own promises.

use std::sync::mpsc;
use std::time::Duration;

use super::{child_pid_fifo, recorded_pid_within};

/// A stub that never records its child's pid ends the wait for it with no
/// pid instead of hanging it: a runtime waits for its blocking tasks before
/// it ends, so the reader blocked on the fifo must be released.
#[test]
fn a_pid_never_recorded_ends_the_wait_instead_of_hanging_it() {
    let dir = std::env::temp_dir().join(format!("yunta-fifo-wait-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fifo = child_pid_fifo(&dir);
    let (sent, ended) = mpsc::channel();
    let waiting = fifo.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let recorded = runtime.block_on(recorded_pid_within(&waiting, Duration::from_millis(50)));
        sent.send(recorded).unwrap();
    });

    let recorded = ended.recv_timeout(Duration::from_secs(10));

    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        recorded,
        Ok(None),
        "the wait ends with no pid instead of hanging"
    );
}
