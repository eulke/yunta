//! Help is painted by the rule every page is.
//!
//! `yunta --help` takes its paint from the policy every other line takes:
//! `--color`, then the environment, decide whether it is painted and in
//! which layer. Nothing else paints it, so a reader who asks for no color
//! gets none, and one who asks for color gets the layer the policy names.
//! clap's own lines (the usage line, the options, and a subcommand's whole
//! help) are painted in the layer the template's lines are. Subcommand help
//! takes the same paint as the root's help. A usage error takes the paint of
//! stderr, which the policy decides for stderr on its own.

use yunta_testkit::{stderr, stdout, yunta_in};

/// What `yunta --help` prints on stdout for `args`, run from a directory of
/// its own with `vars` set on top of the hermetic environment.
fn help(args: &[&str], vars: &[(String, String)]) -> String {
    let away = tempfile::tempdir().unwrap();
    let run = yunta_in!(away.path(), &away.path().join("home"), args, vars);
    assert!(run.status.success(), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("Usage:"), "not the help:\n{text}");
    text
}

/// The variable `name` set to `value`, for [`help`].
fn set(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

/// `--color never` paints no help, even where `CLICOLOR_FORCE` asks for color.
#[test]
fn help_never_paints_when_color_is_never() {
    let text = help(
        &["--color", "never", "--help"],
        &[set("CLICOLOR_FORCE", "1")],
    );
    assert!(
        !text.contains('\u{1b}'),
        "help painted under --color never:\n{text:?}"
    );
}

/// `--color always` paints the help even in a pipe, where the policy would
/// otherwise leave it plain.
#[test]
fn help_always_paints_when_color_is_always() {
    let text = help(&["--color", "always", "--help"], &[]);
    assert!(
        text.contains('\u{1b}'),
        "help unpainted under --color always:\n{text:?}"
    );
}

/// `YUNTA_LOOK=24` paints the help in 24-bit color, as it paints every other
/// line: a row the template writes and a heading clap draws both take it.
#[test]
fn help_paints_in_24_bits_when_look_asks_for_them() {
    let text = help(&["--color", "always", "--help"], &[set("YUNTA_LOOK", "24")]);
    assert!(
        text.contains("\u{1b}[1;38;2;255;255;255myunta init"),
        "help has no strong 24-bit row for `yunta init` under YUNTA_LOOK=24:\n{text:?}"
    );
    assert!(
        text.contains("\u{1b}[38;2;142;136;127mUsage:"),
        "help has no muted 24-bit `Usage:` heading under YUNTA_LOOK=24:\n{text:?}"
    );
}

/// COLORTERM=truecolor gives the help 24-bit color, as it gives every line.
#[test]
fn colorterm_truecolor_paints_help_in_24_bits() {
    let text = help(
        &["--color", "always", "--help"],
        &[set("COLORTERM", "truecolor")],
    );
    assert!(
        text.contains("\u{1b}[1;38;2;255;255;255myunta init"),
        "help has no strong 24-bit row for `yunta init` under COLORTERM=truecolor:\n{text:?}"
    );
    assert!(
        text.contains("\u{1b}[38;2;142;136;127mUsage:"),
        "help has no muted 24-bit `Usage:` heading under COLORTERM=truecolor:\n{text:?}"
    );
}

/// A light ground gets the help in the colors strong text takes on it.
#[test]
fn a_light_ground_paints_help_in_its_own_colors() {
    let text = help(
        &["--color", "always", "--help"],
        &[set("YUNTA_LOOK", "24"), set("COLORFGBG", "0;15")],
    );
    assert!(
        text.contains("\u{1b}[1;38;2;14;12;18myunta init"),
        "help has no strong `yunta init` row in a light ground's colors, COLORFGBG=0;15:\n{text:?}"
    );
    assert!(
        text.contains("\u{1b}[38;2;123;117;111mUsage:"),
        "help has no muted `Usage:` heading in a light ground's colors, COLORFGBG=0;15:\n{text:?}"
    );
}

/// A help in a pipe, with no color asked for by the flag or the environment,
/// is plain.
#[test]
fn help_is_plain_in_a_pipe_that_asks_for_no_color() {
    let text = help(&["--help"], &[]);
    assert!(
        !text.contains('\u{1b}'),
        "help painted in a pipe with no color asked for:\n{text:?}"
    );
}

/// `--color=never` is read the way `--color never` is.
#[test]
fn help_takes_the_equals_form_of_color() {
    let text = help(&["--color=never", "--help"], &[set("CLICOLOR_FORCE", "1")]);
    assert!(
        !text.contains('\u{1b}'),
        "help painted under --color=never:\n{text:?}"
    );
}

/// `NO_COLOR` reaches the help under `auto`, where `CLICOLOR_FORCE` would
/// otherwise paint it.
#[test]
fn no_color_reaches_help_under_auto() {
    let text = help(
        &["--help"],
        &[set("NO_COLOR", "1"), set("CLICOLOR_FORCE", "1")],
    );
    assert!(
        !text.contains('\u{1b}'),
        "help painted under NO_COLOR:\n{text:?}"
    );
}

/// `CLICOLOR_FORCE` paints the help under `auto`, even in a pipe.
#[test]
fn clicolor_force_paints_help_under_auto() {
    let text = help(&["--help"], &[set("CLICOLOR_FORCE", "1")]);
    assert!(
        text.contains('\u{1b}'),
        "help unpainted under CLICOLOR_FORCE:\n{text:?}"
    );
}

/// What `yunta` prints on stderr for `args`, which it refuses, run as
/// [`help`] runs it.
fn refusal(args: &[&str], vars: &[(String, String)]) -> String {
    let away = tempfile::tempdir().unwrap();
    let run = yunta_in!(away.path(), &away.path().join("home"), args, vars);
    assert!(
        !run.status.success(),
        "accepted {args:?}:\n{}",
        stdout(&run)
    );
    stderr(&run)
}

/// A subcommand's help takes the policy the root's help takes.
#[test]
fn subcommand_help_takes_the_same_policy() {
    let painted = help(&["--color", "always", "run", "--help"], &[]);
    assert!(
        painted.contains('\u{1b}'),
        "subcommand help unpainted under --color always:\n{painted:?}"
    );
    let plain = help(
        &["--color", "never", "run", "--help"],
        &[set("CLICOLOR_FORCE", "1")],
    );
    assert!(
        !plain.contains('\u{1b}'),
        "subcommand help painted under --color never:\n{plain:?}"
    );
}

/// A subcommand's help takes the 24-bit layer YUNTA_LOOK asks for.
#[test]
fn subcommand_help_takes_the_24_bit_layer() {
    let text = help(
        &["--color", "always", "run", "--help"],
        &[set("YUNTA_LOOK", "24")],
    );
    assert!(
        text.contains("\u{1b}[38;2;142;136;127mUsage:"),
        "subcommand help has no muted 24-bit `Usage:` heading under YUNTA_LOOK=24:\n{text:?}"
    );
}

/// A usage error takes the policy on stderr: painted under `--color always`,
/// plain under `--color never`.
#[test]
fn a_refusal_takes_the_same_policy_on_stderr() {
    let painted = refusal(&["--color", "always", "--no-such-flag"], &[]);
    assert!(
        painted.contains('\u{1b}'),
        "refusal unpainted under --color always:\n{painted:?}"
    );
    let plain = refusal(
        &["--color", "never", "--no-such-flag"],
        &[set("CLICOLOR_FORCE", "1")],
    );
    assert!(
        !plain.contains('\u{1b}'),
        "refusal painted under --color never:\n{plain:?}"
    );
}
