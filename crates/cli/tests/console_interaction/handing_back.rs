//! What a prompt does to the terminal and the run around it, and what it hands back.

use super::*;

#[test]
fn a_console_prompt_does_not_stall_the_run_tools_listener() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");

    // While the prompt waits for an answer, a SIGINT must still be
    // handled: the Ctrl-C task shares the run's runtime with the console
    // read and the run-tools listener. A read that blocked the runtime
    // would starve every one of them, and this note would never print.
    terminal.interrupt();
    terminal.wait_for(
        "interrupt received",
        "SIGINT went unhandled while the console prompt waited — the read stalled the runtime",
    );
}

#[test]
fn the_live_region_comes_off_the_terminal_before_a_prompt_draws_on_it() {
    let root = tempfile::tempdir().unwrap();
    let terminal = gated(root.path());
    terminal.wait_for("nodes ", "the run never pinned its region to the terminal");
    terminal.wait_on_screen(POINTED, "the gate never put its decision on the console");

    // The region and the prompt write to the same stream, and the one
    // that draws second lands on what the other put there. So the
    // region comes down first, and stays down until the prompt ends.
    assert!(
        terminal.cleared_before("nodes ", POINTED),
        "the region was still pinned to the terminal the prompt drew on:\n{}",
        terminal.drawn()
    );
}

#[test]
fn an_interrupt_at_an_open_prompt_stops_the_run_and_the_process_with_it() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    terminal.interrupt();

    // A person at the menu is no longer being asked anything. The read
    // stops waiting on them, the engine unwinds the run, and the
    // process leaves — rather than living on around a thread parked on
    // a key nobody is going to press.
    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{drawn}"
    );
    assert!(
        drawn.contains("interrupt received"),
        "the interrupt never reached the run's one cancellation bridge:\n{drawn}"
    );
    // The engine's own teardown ran to completion before the process
    // left: the run's close is on the log and in the block that reports
    // it, the checkout it held is unlocked, and its process tree is no
    // longer registered as running.
    assert!(
        drawn.contains("decision on node `lint`") && drawn.contains("holds its own state on disk"),
        "the run's own close never reached the person who stopped it:\n{drawn}"
    );
    assert!(
        !root.path().join("repo/.git/yunta-none.lock").exists(),
        "the checkout this run held is still locked after it stopped"
    );
    let run_dir = run_dir(root.path()).expect("the run wrote its own directory");
    assert!(
        !run_dir.join("scratch/engine.json").exists(),
        "the run's process tree is still registered as running: {}",
        run_dir.display()
    );
    // The menu was left mid-read: the thread that would have put the
    // cursor back is still waiting on a key, so somebody else has to.
    assert!(
        terminal.cursor_is_back(),
        "the abandoned prompt left the terminal without its cursor:\n{drawn}"
    );
}

/// A terminal that declares itself dumb has said it draws nothing beyond text, so
/// nobody is asked there: the run parks and says the command for each option.
#[test]
fn a_dumb_terminal_is_asked_nothing_and_the_run_parks_with_its_commands() {
    let root = tempfile::tempdir().unwrap();
    let checkout = Checkout::under(root.path())
        .working_in_place()
        .workflow(
            "wf",
            "name: approving\nnodes:\n  - id: ship\n    kind: gate\n    assignee: lead\n    \
             message: \"Ship it?\"\n",
        )
        .committed();
    let mut terminal = yunta_on_terminal!(
        &checkout.repo,
        &checkout.home,
        &["run", "wf.yaml"],
        &[("TERM", "dumb")]
    );
    let drawn = terminal.ended();
    assert_eq!(terminal.exit_code(), Some(3), "{drawn}");
    assert!(!drawn.contains(POINTED), "{drawn}");
    assert!(drawn.contains("resolve-gate"), "{drawn}");
}

#[test]
fn a_prompt_that_ends_on_an_interrupt_leaves_the_terminal_its_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.raw();
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    // The list hides the cursor while it draws. Ending any way but
    // answered or declined and leaving it hidden hands the shell the run
    // returns to a terminal with no cursor, which nothing after it puts
    // back.
    terminal.keys("\x03");
    wait_until(
        || terminal.cursor_is_back(),
        || {
            format!(
                "the prompt ended on an interrupt without putting the cursor back\ndrawn so \
                 far:\n{}",
                terminal.drawn()
            )
        },
    );

    let drawn = terminal.ended();
    assert!(
        terminal.cursor_is_back(),
        "the cursor is put back as often as it is taken away:\n{drawn}"
    );
}

#[test]
fn a_prompt_abandoned_mid_read_leaves_the_terminal_reading_and_echoing_again() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = gated(root.path());
    terminal.wait_on_screen("▸ retry", "the gate never put its card on the console");
    // The read turns the line discipline off for as long as it reads,
    // and this is the moment it has: waiting on it is what makes the
    // interrupt below land on a prompt that holds the terminal.
    wait_until(
        || !terminal.line_discipline_is_back(),
        || {
            format!(
                "the prompt never took the terminal's line discipline\ndrawn so far:\n{}",
                terminal.drawn()
            )
        },
    );

    terminal.interrupt();

    // The read is abandoned where it stands: the thread that would put
    // the line discipline back is still waiting on a key nobody is
    // going to press, so the process does it on its way out. Left as
    // the read left it, the shell this run returns to echoes nothing a
    // person types into it.
    let drawn = terminal.ended();
    assert!(
        terminal.line_discipline_is_back(),
        "the abandoned prompt left the terminal with its echo and line editing \
         off:\n{drawn}"
    );
}

#[test]
fn a_typed_ctrl_c_stops_the_run_the_prompts_raw_mode_hid_it_from() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = asking(
        root.path(),
        "questions:\n  - id: summary\n    text: \"What changed?\"\n    answer_type: text\n    \
         required: true\n",
    );
    terminal.raw();
    terminal.wait_on_screen("◆ What changed?", "the question was never asked");
    // Nothing but the prompt itself can put this byte back on the run's
    // one cancellation path now.
    terminal.keys("\x03");
    terminal.wait_for(
        "interrupt received",
        "a typed Ctrl-C was swallowed by the prompt instead of stopping the run",
    );

    let drawn = terminal.ended();
    assert!(
        !terminal.ran_to_the_end(),
        "a run stopped by a person is not a success:\n{drawn}"
    );
    assert_eq!(
        answers(root.path()),
        None,
        "a round a person stopped records nothing at all"
    );
}
