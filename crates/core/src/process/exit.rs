//! Hearing that a child exited the moment it does, without reaping it.
//!
//! The kernel says when a process exits: a pidfd turns readable on Linux,
//! and a kqueue delivers `NOTE_EXIT` on macOS. Either wakes the watcher on
//! the runtime's own event loop, with no thread of its own and no timer,
//! and every wake is confirmed with `waitid(WNOWAIT)`, which leaves the
//! child waitable. The kernel tells of the exit a moment before `waitid`
//! can see it, and tells only once: a confirmation that comes too early
//! is followed by a look every millisecond until the child shows exited.
//! Where neither is available the watcher looks at a steady interval
//! instead.

use std::io;
use std::time::Duration;

use crate::process::group::child_has_exited_unreaped;
use crate::Pid;

/// Waits for one child of this process to exit. The caller keeps the
/// child unreaped while it watches: its pid names it until it is reaped.
pub struct ExitWatch {
    pid: Pid,
    signal: Signal,
}

enum Signal {
    #[cfg(target_os = "linux")]
    Pidfd(tokio::io::unix::AsyncFd<std::os::fd::OwnedFd>),
    #[cfg(target_os = "macos")]
    Kqueue(tokio::io::unix::AsyncFd<kqueue::Queue>),
    Interval(tokio::time::Interval),
}

impl ExitWatch {
    /// Watches `pid`, a child of this process that has not been reaped.
    /// Must be called from within a runtime.
    pub fn new(pid: Pid) -> Self {
        Self {
            pid,
            signal: Signal::of(pid),
        }
    }

    /// Answers once the child has exited, leaving it waitable.
    ///
    /// The child is looked at before the first wait: one that exited
    /// before the kernel was asked to tell has nothing left to tell.
    pub async fn exited(&mut self) -> io::Result<()> {
        loop {
            if child_has_exited_unreaped(self.pid)? {
                return Ok(());
            }
            self.signal.next().await?;
        }
    }
}

impl Signal {
    fn of(pid: Pid) -> Self {
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = pidfd(pid) {
            return Signal::Pidfd(pidfd);
        }
        #[cfg(target_os = "macos")]
        if let Some(queue) = kqueue::watching(pid) {
            return Signal::Kqueue(queue);
        }
        Signal::every(Duration::from_millis(10))
    }

    fn every(period: Duration) -> Self {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Signal::Interval(interval)
    }

    /// Waits for the next moment the child may have exited. Once the
    /// kernel told of the exit, the next moments come every millisecond:
    /// it tells once, and `waitid` sees the exit a moment after.
    async fn next(&mut self) -> io::Result<()> {
        let told = match self {
            #[cfg(target_os = "linux")]
            Signal::Pidfd(pidfd) => {
                pidfd.readable().await?.retain_ready();
                true
            }
            #[cfg(target_os = "macos")]
            Signal::Kqueue(queue) => {
                let mut ready = queue.readable().await?;
                ready.get_inner().drain();
                ready.clear_ready();
                true
            }
            Signal::Interval(interval) => {
                interval.tick().await;
                false
            }
        };
        if told {
            *self = Signal::every(Duration::from_millis(1));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn pidfd(pid: Pid) -> Option<tokio::io::unix::AsyncFd<std::os::fd::OwnedFd>> {
    let pid = rustix::process::Pid::from_raw(pid.as_i32())?;
    let fd = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::NONBLOCK).ok()?;
    tokio::io::unix::AsyncFd::with_interest(fd, tokio::io::Interest::READABLE).ok()
}

#[cfg(target_os = "macos")]
mod kqueue {
    use std::os::fd::{AsFd, AsRawFd, RawFd};

    use nix::libc::timespec;
    use nix::sys::event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue};
    use tokio::io::unix::AsyncFd;

    use crate::Pid;

    /// A kqueue that hears one process exit, and is readable once it has.
    pub(super) struct Queue(Kqueue);

    impl AsRawFd for Queue {
        fn as_raw_fd(&self) -> RawFd {
            self.0.as_fd().as_raw_fd()
        }
    }

    impl Queue {
        /// Takes the notification the queue holds, so it reads as not
        /// ready again until another arrives.
        pub(super) fn drain(&self) {
            let mut taken = [exit_of(0, EvFlags::empty())];
            if let Err(error) = self.0.kevent(&[], &mut taken, Some(NOW)) {
                tracing::debug!(%error, "could not take the exit notification a kqueue held");
            }
        }
    }

    const NOW: timespec = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    fn exit_of(pid: usize, flags: EvFlags) -> KEvent {
        KEvent::new(
            pid,
            EventFilter::EVFILT_PROC,
            flags,
            FilterFlag::NOTE_EXIT,
            0,
            0,
        )
    }

    /// A queue registered for `pid`'s exit. `None` when the queue cannot
    /// be made or the kernel refuses the registration — a process that
    /// already exited is refused — and the caller looks instead.
    pub(super) fn watching(pid: Pid) -> Option<AsyncFd<Queue>> {
        let queue = Kqueue::new().ok()?;
        let watched = usize::try_from(pid.as_i32()).ok()?;
        queue
            .kevent(
                &[exit_of(watched, EvFlags::EV_ADD | EvFlags::EV_ONESHOT)],
                &mut [],
                Some(NOW),
            )
            .ok()?;
        AsyncFd::with_interest(Queue(queue), tokio::io::Interest::READABLE).ok()
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    use super::*;
    use crate::process::signal::{liveness, Liveness};

    fn spawned(script: &str) -> (std::process::Child, Pid) {
        let child = Command::new("sh")
            .args(["-c", script])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = Pid::try_from(child.id()).unwrap();
        (child, pid)
    }

    #[tokio::test]
    async fn a_child_that_exits_later_is_heard_and_stays_waitable() {
        let (mut child, pid) = spawned("sleep 0.2");
        let mut watch = ExitWatch::new(pid);

        tokio::time::timeout(Duration::from_secs(5), watch.exited())
            .await
            .expect("the exit is heard")
            .unwrap();

        assert!(child_has_exited_unreaped(pid).unwrap());
        assert_eq!(
            liveness(pid),
            Liveness::Alive,
            "the child is still waitable"
        );
        child.wait().unwrap();
    }

    /// Children that exit at every moment around the watch's start —
    /// before it registers, while it does, after — are each heard.
    #[tokio::test]
    async fn every_short_lived_child_is_heard() {
        for _ in 0..300 {
            let (mut child, pid) = spawned("true");
            let mut watch = ExitWatch::new(pid);

            tokio::time::timeout(Duration::from_secs(5), watch.exited())
                .await
                .expect("the exit is heard")
                .unwrap();

            child.wait().unwrap();
        }
    }

    #[tokio::test]
    async fn a_child_that_already_exited_is_heard_at_once() {
        let (mut child, pid) = spawned("exit 0");
        while !child_has_exited_unreaped(pid).unwrap() {
            tokio::task::yield_now().await;
        }
        let mut watch = ExitWatch::new(pid);

        tokio::time::timeout(Duration::from_secs(5), watch.exited())
            .await
            .expect("the exit is heard")
            .unwrap();

        assert_eq!(
            liveness(pid),
            Liveness::Alive,
            "the child is still waitable"
        );
        child.wait().unwrap();
    }
}
