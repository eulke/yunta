use super::compared;
use crate::components::{Against, Part};

/// `planned` read against `today` in a Rust file, each planned line drawn
/// with git's word-diff marks. Whatever the case, what each line keeps and
/// puts in is that planned line, and there are as many lines as the plan
/// has.
fn drawn(planned: &str, today: &str) -> Vec<String> {
    let Against::Modified(Some(marks)) = compared("src/lib.rs", planned, today) else {
        panic!("`{planned}` against `{today}` reads as modified, with its marks");
    };
    let lines: Vec<&str> = planned.split('\n').collect();
    assert_eq!(
        marks.lines.len(),
        lines.len(),
        "a line of marks a planned line"
    );
    for (parts, line) in marks.lines.iter().zip(&lines) {
        let written: String = parts
            .iter()
            .filter_map(|part| match part {
                Part::Kept(text) | Part::Added(text) => Some(text.as_str()),
                Part::Removed(_) => None,
            })
            .collect();
        assert_eq!(&written, line, "what a line keeps and puts in is the line");
    }
    marks
        .lines
        .iter()
        .map(|parts| {
            parts
                .iter()
                .map(|part| match part {
                    Part::Kept(text) => text.clone(),
                    Part::Added(text) => format!("{{+{text}+}}"),
                    Part::Removed(text) => format!("[-{text}-]"),
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_parameter_put_in_is_marked_between_the_commas_that_part_it() {
    assert_eq!(
        drawn("fn f(a: u8, b: u8) -> u8;", "fn f(a: u8) -> u8;"),
        ["fn f(a: u8, {+b: u8+}) -> u8;"]
    );
}

#[test]
fn a_last_parameter_taken_out_takes_the_comma_before_it() {
    assert_eq!(
        drawn("fn f(a: u8);", "fn f(a: u8, b: u8);"),
        ["fn f(a: u8[-, b: u8-]);"]
    );
}

#[test]
fn a_first_parameter_taken_out_takes_the_comma_after_it() {
    assert_eq!(
        drawn("fn f(b: u8);", "fn f(a: u8, b: u8);"),
        ["fn f([-a: u8, -]b: u8);"]
    );
}

#[test]
fn a_word_replaced_says_what_it_was_and_what_it_is() {
    assert_eq!(
        drawn("fn f(a: u16);", "fn f(a: u8);"),
        ["fn f(a: [-u8-]{+u16+});"]
    );
}

#[test]
fn signs_left_standing_between_replaced_words_read_as_replaced() {
    assert_eq!(
        drawn("fn g(y: B)", "fn f(x: A)"),
        ["fn [-f(x: A-]{+g(y: B+})"]
    );
}

#[test]
fn a_field_put_in_on_its_own_line_is_marked_on_that_line_alone() {
    assert_eq!(
        drawn(
            "pub struct S {\n    a: u8,\n    b: u8,\n}",
            "pub struct S {\n    a: u8,\n}"
        ),
        ["pub struct S {", "    a: u8,", "    {+b: u8+},", "}"]
    );
}

/// A `;` ends an item: a method put in between two others on one line is
/// marked from the word after one `;` to its own.
#[test]
fn a_method_put_in_between_two_on_one_line_is_marked_whole() {
    assert_eq!(
        drawn(
            "trait T { fn a(&self); fn b(&self); fn c(&self); }",
            "trait T {\n    fn a(&self);\n    fn c(&self);\n}"
        ),
        ["trait T { fn a(&self); {+fn b(&self);+} fn c(&self); }"]
    );
}

#[test]
fn a_statement_put_in_between_two_is_marked_whole() {
    assert_eq!(
        drawn("fn f() { a(); b(); c(); }", "fn f() { a(); c(); }"),
        ["fn f() { a(); {+b();+} c(); }"]
    );
}

/// A `}` ends an item too, whatever follows it.
#[test]
fn an_item_put_in_after_a_block_is_marked_whole() {
    assert_eq!(
        drawn("impl A { fn a() {} fn b() {} }", "impl A { fn a() {} }"),
        ["impl A { fn a() {} {+fn b() {}+} }"]
    );
    assert_eq!(
        drawn(
            "impl A { fn a() {} fn b() {} fn c() {} }",
            "impl A { fn a() {} fn c() {} }"
        ),
        ["impl A { fn a() {} {+fn b() {}+} fn c() {} }"]
    );
}

#[test]
fn a_method_taken_out_between_two_on_one_line_is_marked_whole() {
    assert_eq!(
        drawn(
            "trait T { fn a(&self); fn c(&self); }",
            "trait T { fn a(&self); fn b(&self); fn c(&self); }"
        ),
        ["trait T { fn a(&self);[-fn b(&self);-] fn c(&self); }"]
    );
}

#[test]
fn line_breaks_comments_and_trailing_commas_are_not_compared() {
    assert_eq!(
        compared(
            "src/lib.rs",
            "pub enum E { A, B }",
            "/// Two of them.\npub enum E {\n    /// The first.\n    A,\n    B,\n}"
        ),
        Against::Unchanged
    );
    assert_eq!(
        compared("src/lib.rs", "/// Says hello.\nfn f();", "fn f();"),
        Against::Unchanged
    );
}

#[test]
fn a_string_is_one_word() {
    assert_eq!(
        drawn("const S: &str = \"a b\";", "const S: &str = \"a c\";"),
        ["const S: &str = [-\"a c\"-]{+\"a b\"+};"]
    );
}

#[test]
fn a_root_a_plan_puts_in_a_signature_is_the_one_thing_marked() {
    assert_eq!(
        drawn(
            include_str!("fixtures/resolve_workflow.planned.txt").trim_end(),
            include_str!("fixtures/resolve_workflow.today.txt").trim_end()
        ),
        ["pub fn resolve_workflow(repo_root: &Path, {+global_packs_root: &Path+}, name: &str) -> Result<ResolvedWorkflow, CatalogError>;"]
    );
}

/// Today's enum is written one field to a line, each under its doc comment;
/// the plan writes each variant on one line, with a flag more.
#[test]
fn a_flag_a_plan_puts_in_each_variant_is_marked_in_each_and_nothing_else() {
    assert_eq!(
        drawn(
            include_str!("fixtures/pack_action.planned.txt").trim_end(),
            include_str!("fixtures/pack_action.today.txt").trim_end()
        ),
        [
            "#[derive(Subcommand)]",
            "enum PackAction {",
            "    Add { source: String, {+#[arg(long)] global: bool+}, #[arg(long)] yes: bool, #[arg(long)] run_tests: bool },",
            "    Update { pack: PackRef, r#ref: String, {+#[arg(long)] global: bool+}, #[arg(long)] yes: bool },",
            "    New { pack: PackRef },",
            "    Remove { pack: PackRef, {+#[arg(long)] global: bool+} },",
            "    List {+{ #[arg(long)] global: bool }+},",
            "    Audit { pack: PackRef, {+#[arg(long)] global: bool+} },",
            "}",
        ]
    );
}

#[test]
fn two_codes_too_long_to_compare_word_by_word_differ_without_their_marks() {
    let words = |mark: char, count: usize| -> String {
        (0..count)
            .map(|at| format!("{mark}{at}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(
        compared("src/lib.rs", &words('a', 1100), &words('b', 1100)),
        Against::Modified(None)
    );
    assert!(
        matches!(
            compared("src/lib.rs", &words('a', 1000), &words('b', 1000)),
            Against::Modified(Some(_))
        ),
        "a thousand words against a thousand are compared"
    );
}
