//! See [`super`]. The process groups a run's engine registered, closed in a
//! session's own order: the groups under each registered group while its CLI
//! still lives, SIGKILL to the registered groups, then whatever still
//! carries a mark of the run's sessions, such as a command whose parent
//! left. A group that is already gone is the end state wanted; anything else
//! that would not stop is said, never hidden. A group that is no run's own
//! (see [`signalable_group`]) is skipped with a remark rather than walked or
//! signalled.

use std::path::Path;

use yunta_core::process::group::force_kill_group;
use yunta_core::process::mark::{close_marked, SessionMark};
use yunta_core::process::signal::{signal_group, Signal};
use yunta_core::process::tree::close_groups_under;
use yunta_core::{describe, Pid, RunId};

use crate::{Remark, Remarks};

/// Closes what the engine of `run` left in `groups`: the groups under each,
/// SIGKILL to each, then what carries a mark of the run's sessions.
pub(super) async fn kill(run: &RunId, groups: &[Pid], remarks: &dyn Remarks) {
    let groups = signalable(groups, remarks);
    close_under(&groups, remarks).await;
    for pgid in &groups {
        if let Err(error) = signal_group(*pgid, Signal::SIGKILL) {
            remarks.remark(Remark::Unstopped {
                said: describe(&error),
            });
        }
    }
    close_marks(run, remarks).await;
}

/// SIGKILL to every process group the registry under `run_dir` names, in the
/// crash path's order: the groups a stopped run's engine could not stop,
/// which hold the run's checkout taken while they run. The groups under each
/// are closed first, and what carries a mark of the run's sessions last. A
/// group whose leader has exited is signalled too, since its members may
/// still run. Each group is killed until none of its members can run.
/// Answers how many of the named groups still answered when it looked.
pub(super) async fn kill_unstopped(run_dir: &Path, run: &RunId, remarks: &dyn Remarks) -> usize {
    let yunta_engine::Registry::Read(registry) = yunta_engine::read_registry(run_dir) else {
        return 0;
    };
    let groups = &registry.doc.process_groups;
    let running = answering(groups);
    let groups = signalable(groups, remarks);
    close_under(&groups, remarks).await;
    for pgid in groups {
        if let Err(error) = force_kill_group(pgid).await {
            remarks.remark(Remark::Unstopped {
                said: describe(&error),
            });
        }
    }
    close_marks(run, remarks).await;
    running
}

/// The groups of `groups` that may be signalled as a run's own; each one
/// that may not is said once, and skipped.
fn signalable(groups: &[Pid], remarks: &dyn Remarks) -> Vec<Pid> {
    groups
        .iter()
        .copied()
        .filter(|pgid| {
            let admitted = signalable_group(*pgid);
            if !admitted {
                remarks.remark(Remark::GroupRefused { pgid: *pgid });
            }
            admitted
        })
        .collect()
}

/// Closes every group under each of `groups`, found by parent links while
/// the CLI leading it still lives. What would not close is said.
async fn close_under(groups: &[Pid], remarks: &dyn Remarks) {
    for group in groups {
        if let Err(error) = close_groups_under(*group).await {
            remarks.remark(Remark::Unstopped {
                said: error.to_string(),
            });
        }
    }
}

/// Closes whatever carries a mark of `run`'s sessions, wherever its parent
/// is: what left the registered groups' trees. What would not close is said.
async fn close_marks(run: &RunId, remarks: &dyn Remarks) {
    if let Err(error) = close_marked(&SessionMark::of_run(run)).await {
        remarks.remark(Remark::Unstopped {
            said: error.to_string(),
        });
    }
}

/// How many of `groups` still run, read as the engine reads them: a group
/// whose leader exited while a child it left runs still counts.
fn answering(groups: &[Pid]) -> usize {
    yunta_engine::running_groups(groups).len()
}

/// Whether a cancel may signal a process group. `signal_group` is
/// `killpg(pgid)` = `kill(-pgid)`, so `pgid` of 1 becomes `kill(-1)`: every
/// process this user can signal, not one run's tree. The [`Pid`] type
/// already makes 0 and negatives unrepresentable, so this rejects the one
/// dangerous value that survives it: init's group (pgid 1). A registry
/// carrying it is corrupt, and the safe reading is to skip it.
fn signalable_group(pgid: Pid) -> bool {
    pgid.as_u32() > 1
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use yunta_core::Pid;

    use super::{signalable, signalable_group};
    use crate::Remark;

    /// The groups a registry names are walked and signalled only once
    /// `signalable` admits them, so init's group is neither walked nor
    /// signalled, and the run's own groups are both.
    #[test]
    fn init_s_group_is_neither_walked_nor_signalled() {
        let named = [
            Pid::try_from(1u32).unwrap(),
            Pid::try_from(4321u32).unwrap(),
        ];
        let said = Mutex::new(Vec::new());
        let admitted = signalable(&named, &|remark| said.lock().unwrap().push(remark));
        assert_eq!(admitted, vec![named[1]]);
        let said = said.into_inner().unwrap();
        assert!(
            matches!(said.as_slice(), [Remark::GroupRefused { pgid }] if *pgid == named[0]),
            "{said:?}"
        );
    }

    #[test]
    fn a_run_never_signals_init_or_a_lower_group() {
        // pgid 1 -> killpg(1) -> kill(-1): every process, not one run's
        // tree. `Pid` forbids 0 and negatives, so 1 is the sole dangerous
        // value it still admits, and the one this must reject.
        assert!(!signalable_group(Pid::try_from(1u32).unwrap()));
        assert!(signalable_group(Pid::try_from(2u32).unwrap()));
        assert!(signalable_group(Pid::try_from(4321u32).unwrap()));
    }
}
