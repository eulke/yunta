use super::*;

#[test]
fn inner_space_runs_are_the_trace_of_a_lost_continuation() {
    assert!(has_inner_space_run(
        r#"    "the adapter declares no                 session resume""#
    ));
    // A few spaces align a column; the trace is a source line's indentation.
    assert!(!has_inner_space_run(
        r#"    "  {} {:>3} run(s)    median CPTV {}    median tokens {}","#
    ));
    // Indentation a literal opens with is layout, not a trace.
    assert!(!has_inner_space_run(
        r#"    println!("      tradeoff: {}", x);"#
    ));
    // So is indentation after a `\n` escape, and a comment is not a message.
    assert!(!has_inner_space_run(
        r#"    "add e.g.:\n      runners:\n        \"#
    ));
    assert!(!has_inner_space_run("    // a          comment"));
    assert!(!has_inner_space_run("    let x = 1;"));
}

#[test]
fn a_debug_form_in_a_message_counts_and_one_for_a_developer_does_not() {
    assert!(has_debug_format(
        r#"        .map(|(id, status)| format!("{id}: {status:?}"))"#
    ));
    assert!(has_debug_format(
        r#"    #[error("failed to read {stream:?} for `{command}`")]"#
    ));
    assert!(has_debug_format(
        r#"                "{} at or above {max_severity:?}: {}","#
    ));
    assert!(!has_debug_format(
        r#"        assert_eq!(a, b, "{drawn:?}");"#
    ));
    assert!(!has_debug_format(
        r#"        other => panic!("expected it, got {other:?}"),"#
    ));
    assert!(!has_debug_format(
        r#"    tracing::debug!("state {state:?}");"#
    ));
    assert!(!has_debug_format("    // prints {value:?} for now"));
    assert!(!has_debug_format(r#"    format!("{value}")"#));
}

#[test]
fn a_parenthesized_plural_in_a_string_counts_and_one_in_a_comment_does_not() {
    assert!(has_parenthesized_plural(
        r#"    "scope violated: {} file(s) outside the declared globs","#
    ));
    // A continued line of a message opens on the word.
    assert!(has_parenthesized_plural(
        r#"             artifact(s) it names: {}","#
    ));
    assert!(has_parenthesized_plural(r#"    "{} re-route(s)","#));
    assert!(has_parenthesized_plural(r#"    "{} child(ren) finished","#));
    assert!(!has_parenthesized_plural(r#"    "call (with care)","#));
    assert!(!has_parenthesized_plural("    // every file(s) it names"));
    assert!(!has_parenthesized_plural(
        "    Value::String(s) => s.clone(),"
    ));
    assert!(!has_parenthesized_plural("    let t = str::trim(s);"));
    assert!(!has_parenthesized_plural("    let t = Some(s);"));
}

#[test]
fn a_dash_in_a_string_counts_and_one_in_a_comment_does_not() {
    assert!(has_dash("    \"node `a` failed \u{2014} exit 1\","));
    // A continued line of a message opens on the word.
    assert!(has_dash("             \u{2013} a range\","));
    assert!(!has_dash("    /// a claim \u{2014} and what explains it"));
    assert!(!has_dash(
        "    call(); // a claim \u{2014} and what explains it"
    ));
    assert!(!has_dash("    \"node `a` failed: exit 1\","));
}

#[test]
fn a_dash_in_a_doc_comment_counts_where_it_is_help() {
    assert!(has_doc_dash("    /// Runs a workflow \u{2014} to its end."));
    assert!(!has_doc_dash("    /// Runs a workflow: to its end."));
    assert!(!has_doc_dash(
        "    // a note \u{2014} for whoever reads the code"
    ));
}

#[test]
fn functions_over_counts_only_bodies_past_the_budget() {
    let short = "fn a() {\n    let x = 1;\n}\n";
    assert_eq!(functions_over(short, 5), 0);
    let long = format!("fn b() {{\n{}}}\n", "    let x = 1;\n".repeat(10));
    assert_eq!(functions_over(&long, 5), 1);
}

#[test]
fn functions_over_ignores_a_brace_inside_a_string() {
    // The `{` in the string must not open a body, and the trait method
    // declaration with no body must not count.
    let source = "fn a() -> &'static str {\n    \"} not a brace {\"\n}\ntrait T { fn m(&self); }\n";
    assert_eq!(functions_over(source, 0), 1);
}

#[test]
fn sync_fs_counts_only_the_calls_inside_an_async_body() {
    let source = "\
async fn reads() {
    let text = std::fs::read_to_string(path);
}
fn also_reads() {
    let text = std::fs::read_to_string(path);
}
";
    assert_eq!(sync_fs_in_async(source), 1);
}

#[test]
fn a_justified_or_offloaded_disk_call_in_an_async_body_is_not_counted() {
    let justified = "async fn g() {\n    // blocking: atomic create, so the lock is whole\n    let _ = std::fs::File::create(\"b\");\n}\n";
    assert_eq!(sync_fs_in_async(justified), 0);
    let offloaded = "async fn h() {\n    tokio::task::spawn_blocking(move || std::fs::read_to_string(\"c\")).await.ok();\n}\n";
    assert_eq!(sync_fs_in_async(offloaded), 0);
    assert_eq!(
        sync_fs_in_async("async fn f() {\n    let _ = std::fs::read(\"a\");\n}\n"),
        1
    );
}

#[test]
fn a_test_module_whose_braces_never_balance_runs_to_the_end_of_the_file() {
    let source = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        docs/**\n}\n";
    assert_eq!(
        test_mod_mask(source),
        vec![false, true, true, true, true, true]
    );
}

#[test]
fn test_mod_mask_covers_a_cfg_test_module() {
    let source = "fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\nfn also_prod() {}\n";
    let mask = test_mod_mask(source);
    assert_eq!(
        mask,
        vec![false, true, true, true, true, false],
        "the attribute, header and body are masked; production lines are not"
    );
    // Production-only text keeps prod, drops the test module's contents.
    let prod = production_only(source);
    assert!(prod.contains("fn prod()") && prod.contains("fn also_prod()"));
    assert!(!prod.contains("fn t()"));
}

#[test]
fn a_raw_string_holding_a_block_comment_opener_hides_no_function() {
    // The quote inside the literal does not close it: only a quote with
    // the opener's one hash does.
    let source = format!(
        "const GLOBS: &str = r#\"\nsrc/** \"quoted\"\n\"#;\nfn long() {{\n{}}}\n",
        "    let step = 1;\n".repeat(52)
    );
    assert_eq!(functions_over(&source, 50), 1);
}

#[test]
fn a_multi_line_string_holding_a_block_comment_opener_hides_no_function() {
    let source = format!(
        "const TASKS: &str = \"\nsrc/**\n\";\nfn long() {{\n{}}}\n",
        "    let step = 1;\n".repeat(52)
    );
    assert_eq!(functions_over(&source, 50), 1);
}

#[test]
fn a_char_literal_holding_a_quote_opens_no_string() {
    let source = format!(
        "fn quote() -> char {{\n    '\"'\n}}\nfn long() {{\n{}}}\n",
        "    let step = 1;\n".repeat(52)
    );
    assert_eq!(functions_over(&source, 50), 1);
}

#[test]
fn a_byte_string_holding_a_glob_hides_no_function() {
    // The escaped quote is a character of the literal, not its end.
    let source = format!(
        "const GLOBS: &[u8] = b\"src/** \\\"quoted\";\nfn long() {{\n{}}}\n",
        "    let step = 1;\n".repeat(52)
    );
    assert_eq!(functions_over(&source, 50), 1);
}

#[test]
fn a_byte_raw_string_holding_a_glob_hides_no_function() {
    // The quotes inside are text: only a quote with the opener's one hash
    // ends the literal.
    let source = format!(
        "const GLOBS: &[u8] = br#\"src/** \"quoted\"#;\nfn long() {{\n{}}}\n",
        "    let step = 1;\n".repeat(52)
    );
    assert_eq!(functions_over(&source, 50), 1);
}

#[test]
fn a_one_line_raw_string_holding_a_block_comment_opener_hides_no_function() {
    // The quote that precedes `open` is text, so the literal ends at the hash.
    let source = format!(
        "const OPENER: &str = r#\"/* \"open\"#;\nfn long() {{\n{}}}\n",
        "    let step = 1;\n".repeat(52)
    );
    assert_eq!(functions_over(&source, 50), 1);
}
