//! When a process started, from the host's process table. Linux
//! publishes it in `/proc/<pid>/stat`; a host without `/proc` (macOS)
//! cannot tell, and the answer is `None` — never a guess.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use yunta_core::Pid;

/// Clock ticks per second in the times `/proc` reports since boot —
/// fixed at 100 in the kernel's user-space ABI, whatever the
/// scheduler's own frequency.
const USER_HZ: u64 = 100;

/// The moment the process with `pid` started, rounded down to the
/// boot time's whole second: never later than the real start.
pub fn process_start(pid: Pid) -> Option<SystemTime> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let ticks = start_ticks(&stat)?;
    Some(boot_time()? + Duration::from_millis(ticks * 1000 / USER_HZ))
}

/// Field 22 of `/proc/<pid>/stat`, ticks after boot. The command name
/// (field 2) sits in parentheses and may contain spaces or
/// parentheses of its own, so fields are counted from its closing
/// parenthesis: the next field is the state (3), and the start time is
/// 19 fields further.
fn start_ticks(stat: &str) -> Option<u64> {
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

/// `btime` from `/proc/stat`: the boot, in whole seconds since the epoch.
fn boot_time() -> Option<SystemTime> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let seconds: u64 = stat
        .lines()
        .find_map(|line| line.strip_prefix("btime "))?
        .trim()
        .parse()
        .ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_time_is_read_past_a_command_name_with_spaces_and_parentheses() {
        let stat = "4242 (a (weird) name) S 1 4242 4242 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 1 0 1974991 12345 678 18446744073709551615";
        assert_eq!(start_ticks(stat), Some(1_974_991));
    }

    #[test]
    fn a_truncated_record_has_no_start_time() {
        assert_eq!(start_ticks("4242 (sh) S 1 4242"), None);
        assert_eq!(start_ticks(""), None);
    }
}
