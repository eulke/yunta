//! The committed baseline and the two ways it is read. `smells` lowers it to
//! what the tree counts and refuses any rise. `smells --check` holds the tree
//! to it and to the baseline of the branch's merge base, so a change that
//! raises a count and the baseline together is still refused.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Each counter's number, by name.
pub(super) type Counts = BTreeMap<String, usize>;

/// The baseline's lines, `<name> <count>` each.
pub(super) fn parse(text: &str) -> Counts {
    text.lines()
        .filter_map(|line| {
            let (name, count) = line.rsplit_once(' ')?;
            Some((name.to_string(), count.trim().parse().ok()?))
        })
        .collect()
}

pub(super) fn render(counts: &Counts) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name} {count}\n"))
        .collect()
}

/// What each counter may count: the lower of the committed baseline and the
/// merge base's. A counter the merge base does not name enters at zero, as
/// `smells` writes it; with no merge base read, the committed number holds.
pub(super) fn ceiling(committed: &Counts, merge_base: Option<&Counts>) -> Counts {
    committed
        .iter()
        .map(|(name, &count)| {
            let held = match merge_base {
                Some(base) => base.get(name).map_or(0, |&based| based.min(count)),
                None => count,
            };
            (name.clone(), held)
        })
        .collect()
}

/// Each count of `counts` above what `allowed` lets it be, with how to bring
/// it down. A counter `allowed` does not name may only stand at zero.
pub(super) fn rises(allowed: &Counts, counts: &Counts) -> Vec<String> {
    counts
        .iter()
        .filter_map(|(name, &count)| {
            let held = allowed.get(name).copied();
            let at_most = held.unwrap_or(0);
            (count > at_most).then(|| {
                format!(
                    "  {name}: at most {at_most}, counts {count} — {}",
                    fix(name)
                )
            })
        })
        .collect()
}

/// The refusal of `risen` against the baseline at `path`.
pub(super) fn refusal(path: &Path, risen: &[String]) -> String {
    format!(
        "these smell counts rose above `{}` — bring each down as it says; a count only \
         falls, and the baseline falls with it:\n{}",
        path.display(),
        risen.join("\n")
    )
}

/// The baseline the merge base of `HEAD` and the branch it merges into
/// committed: `origin/<GITHUB_BASE_REF>` in a pull request, `origin/main`
/// otherwise. A merge base git cannot find is an error in CI, where a check
/// that cannot read it would let a raised baseline through; elsewhere it is
/// `None`, and so is a merge base that holds no baseline yet.
pub(super) fn at_merge_base(root: &Path) -> Result<Option<Counts>, String> {
    let branch = std::env::var("GITHUB_BASE_REF")
        .ok()
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "main".to_string());
    let base = format!("origin/{branch}");
    let commit = match git(root, &["merge-base", "HEAD", &base]) {
        Ok(commit) => commit,
        Err(said) => {
            return match std::env::var_os("CI") {
                Some(_) => Err(format!(
                    "the merge base of HEAD and `{base}` cannot be read ({said}): check out \
                     with the branch's history (`fetch-depth: 0`)"
                )),
                None => Ok(None),
            }
        }
    };
    let committed = format!("{}:xtask/smells.baseline", commit.trim());
    Ok(git(root, &["show", &committed])
        .ok()
        .map(|text| parse(&text)))
}

/// git's stdout in `root` on exit 0, or what it said otherwise.
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("git does not start: {error}"))?;
    match output.status.success() {
        true => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        false => Err(String::from_utf8_lossy(&output.stderr).trim().to_string()),
    }
}

/// What [`fix`] says of a counter it does not know.
pub(super) const UNNAMED_FIX: &str = "bring it back to its baseline";

/// How to bring the counter `name` down.
pub(super) fn fix(name: &str) -> &'static str {
    match name {
        "banned_vocabulary" => "name the thing by its current word; xtask/src/smells/prose.rs lists the retired ones",
        "bench_struct_outside_testkit" => "take the bench from yunta-testkit and the clock from yunta-testkit-core",
        "cli_print_outside_output" => "print through the CLI's output module",
        "cli_word_outside_cli" => "say it in the crate's own terms and let the CLI word the command",
        "copied_test_helpers" => "call the helper the testkit exports: the repository helpers in yunta-testkit, the clocks in yunta-testkit-core",
        "create_run_outside_testkit" => "create the run through the engine's run module or yunta-testkit",
        "debug_format_reaching_reader" => "format the value for its reader, not with `{:?}`",
        "doc_reference" => "say the behaviour and its reason instead of naming a document",
        "engine_fs_outside_runtime" => "use the runtime's file calls, or put a `// blocking:` reason above the call",
        "env_mutation_in_tests" => "inject what the test needs instead of writing the process environment",
        "env_read_outside_boundary" => "read the variable at the boundary and pass it in",
        "event_builder_outside_testkit" => "build the event with yunta-testkit-core's log builder",
        "execute_run_outside_testkit" => "drive the run through the engine's run module or yunta-testkit",
        "exit_failure_in_cli" => "return the error and let `main` map it to an exit status",
        "git_command_outside_git_rs" => "run git through the engine's git module, or in a test through the testkit's git helper",
        "let_underscore_in_prod" => "handle the result, record the degradation it reports, or put a `// best-effort:` reason above it",
        "mode_name_in_prod" => "read what the run's mode declares; the workflow names its modes",
        "module_docs_over_60_lines" => "keep the `//!` to how the module works; move a rule to the test that holds it",
        "numeric_const_without_reason" => "put a `///` line above the constant saying why it has its value",
        "parenthesized_plural_in_prod_strings" => "write the plural the number calls for",
        "prod_fns_over_50_lines" | "prod_fns_over_100_lines" => "split the function where a seam shows",
        "reason_built_by_format" => "record the values as fields and let the surface word them",
        "run_tool_named_outside_vocabulary" => "name the tool through the session vocabulary's rule",
        "separator_literal_outside_glyphs" => "take the glyph from the glyph set",
        "session_request_literal_outside_plan" => "build the request in the session plan",
        "sgr_outside_ink" => "paint through the render crate's ink",
        "sleep_outside_mock_latency" => "wait for the condition instead of sleeping",
        "space_runs_in_prod_strings" => "end the line with `\\` so the next line's indentation stays out of the text",
        "spanish_prose_lines" => "write it in English",
        "src_files_over_500_lines" => "split the file along a module, or move its tests to a `tests.rs` beside it",
        "sync_fs_in_async" => "use the async file call",
        "system_clock_outside_boundary" => "take the time from the injected clock",
        "tense_markers" => "say what the code does, in the present tense; a date or a plan goes in the commit message",
        "test_files_over_500_lines" => "split the test file by the behaviour its tests prove",
        "test_fns_over_50_lines" => "split the test into one test per promise",
        "token_count_formatted_outside_units" => "format the count through the units module",
        "wildcard_in_ledger_apply" => "name each event kind the fold handles; list one that moves nothing in the replay test",
        _ => UNNAMED_FIX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pairs: &[(&str, usize)]) -> Counts {
        pairs
            .iter()
            .map(|(name, count)| (name.to_string(), *count))
            .collect()
    }

    /// A baseline raised in the same change as its count is held to the merge
    /// base's, and a counter the merge base does not name enters at zero.
    #[test]
    fn the_ceiling_is_the_lower_of_the_committed_and_the_merge_base_baselines() {
        let committed = counts(&[("a", 3), ("b", 1), ("new", 2)]);
        let based = counts(&[("a", 2), ("b", 4)]);
        assert_eq!(
            ceiling(&committed, Some(&based)),
            counts(&[("a", 2), ("b", 1), ("new", 0)])
        );
        assert_eq!(ceiling(&committed, None), committed);
    }

    #[test]
    fn a_rise_names_its_counter_its_numbers_and_how_to_bring_it_down() {
        let allowed = counts(&[("doc_reference", 0), ("tense_markers", 2)]);
        let measured = counts(&[("doc_reference", 1), ("tense_markers", 2), ("fresh", 0)]);
        let risen = rises(&allowed, &measured);
        assert_eq!(risen.len(), 1, "{risen:?}");
        assert!(
            risen[0].contains("doc_reference: at most 0, counts 1"),
            "{risen:?}"
        );
        assert!(risen[0].contains("naming a document"), "{risen:?}");
        let unheld = rises(&allowed, &counts(&[("fresh", 2)]));
        assert!(
            unheld[0].contains("fresh: at most 0, counts 2"),
            "{unheld:?}"
        );
    }
}
