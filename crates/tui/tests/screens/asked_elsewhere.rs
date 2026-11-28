//! A run whose process asks a question at its own terminal, held to its landed target at the
//! four sizes: its frames name that process by a placeholder, read as the process the test
//! runs in, which the registry of its cut names.

use surco::motion::Motion;
use yunta_testkit_ui::screens;

use super::{drawn_moving, targets, ASCII_120, UNICODE_120, UNICODE_60, UNICODE_80};

/// Where a target names the process that drives its run: the registry of its cut names the
/// process the test runs in, so the target holds this in place of a number.
const PID: &str = "{pid}";

/// The gist of the run asked elsewhere, its process `pid`, on a row of `cells` as the rule on
/// a cut cuts it, written apart from the code: whole where it fits; else what fits beside the
/// ellipsis, the space before the cut left out, and the cut falling before the process's
/// number where it would split that number or end on it.
fn asking_gist_cut(pid: &str, cells: usize) -> String {
    let gist = format!("node `ship` is asking at its terminal (pid {pid}); answer it there");
    if gist.chars().count() <= cells {
        return gist;
    }
    let number = gist.find(pid).expect("the gist names the process");
    let kept: String = gist.chars().take(cells - 1).collect();
    let end = match kept.len() {
        end if end > number && end <= number + pid.len() => number,
        end => end,
    };
    format!("{}…", kept[..end].trim_end())
}

/// A run whose process asks a question at its own terminal equals its landed target at each
/// of the four sizes, its pane titled by the gate and its assignee: the target's process read
/// as the one this test runs in, which the registry of its cut names. At the least terminal
/// its gist, which names that process, is cut on its row: the target's row is the rule's cut
/// for a number of five digits, and the row drawn is the rule's cut for the number in hand,
/// so a process of any number holds it.
#[test]
fn a_run_asked_elsewhere_equals_its_landed_target_at_every_size() {
    yunta_testkit::on_a_deep_stack(|| async {
        let target = "asked-elsewhere";
        let screen = screens::run::opened(target).await;
        let pid = std::process::id().to_string();
        let gist_cut_at = [
            (&UNICODE_120, None),
            (&UNICODE_80, None),
            (&ASCII_120, None),
            (&UNICODE_60, Some(5)),
        ];
        for (environment, gist) in gist_cut_at {
            let drawn = drawn_moving(screen.state(), environment, Motion::Moves);
            let path = targets().join(format!("{target}.{}.txt", environment.name));
            let landed = std::fs::read_to_string(&path).expect("a target frame");
            let mut rows: Vec<String> = landed.lines().map(|row| row.replace(PID, &pid)).collect();
            if let Some(gist) = gist {
                let cells = usize::from(environment.columns) - 2;
                let held = landed.lines().nth(gist - 1).expect("the gist's row");
                assert_eq!(held, format!(" {}", asking_gist_cut(PID, cells)));
                rows[gist - 1] = format!(" {}", asking_gist_cut(&pid, cells));
            }
            let expected: String = rows.iter().map(|row| format!("{row}\n")).collect();
            assert!(
                drawn == expected,
                "{target} at {} differs from {}, the process {pid}:\n--- drawn\n{drawn}--- \
                 target\n{expected}",
                environment.name,
                path.display()
            );
        }
    });
}
