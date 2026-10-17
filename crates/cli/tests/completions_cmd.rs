//! `yunta completions <shell>`: a script every shell completes the
//! commands with, read off the same command tree the help is.

use yunta_testkit::{stdout, yunta_in};

/// The subcommands `yunta --help` groups.
fn subcommands(dir: &std::path::Path) -> Vec<String> {
    let help = stdout(&yunta_in!(dir, &dir.join("home"), &["--help"]));
    let mut names = Vec::new();
    let mut in_group = false;
    for line in help.lines() {
        if line.trim().is_empty() {
            in_group = false;
        } else if !line.starts_with(' ') && line.ends_with(':') {
            in_group = !matches!(line, "Options:" | "Examples:");
        } else if in_group {
            names.extend(line.split_whitespace().next().map(str::to_string));
        }
    }
    names
}

#[test]
fn completions_name_every_subcommand_for_each_shell() {
    let away = tempfile::tempdir().unwrap();
    let names = subcommands(away.path());
    assert!(names.len() > 10, "the help lists the commands: {names:?}");
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let script = yunta_in!(
            away.path(),
            &away.path().join("home"),
            &["completions", shell]
        );
        assert!(script.status.success(), "{shell}");
        let text = stdout(&script);
        for name in &names {
            assert!(
                text.contains(name.as_str()),
                "{shell} completes no `{name}`"
            );
        }
    }
}
