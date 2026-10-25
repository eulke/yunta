use super::as_planned;

/// A function with a doc comment, an attribute and a body, its parameters
/// laid out a line each.
const BUILD_IT: &str =
    "/// Doc.\n#[inline]\npub fn build_it(\n    depth: u32,\n) -> u8 {\n    1\n}\n";

#[test]
fn a_function_planned_to_its_signature_reads_today_s_signature() {
    assert_eq!(
        as_planned(BUILD_IT, "pub fn build_it(depth: u32, width: u32) -> u8;").as_deref(),
        Some("pub fn build_it(\n    depth: u32,\n) -> u8;")
    );
}

#[test]
fn today_s_attributes_are_read_where_the_plan_writes_some() {
    assert_eq!(
        as_planned(BUILD_IT, "#[inline]\npub fn build_it(depth: u32) -> u8;").as_deref(),
        Some("#[inline]\npub fn build_it(\n    depth: u32,\n) -> u8;")
    );
}

#[test]
fn a_function_planned_with_its_body_reads_whole() {
    assert_eq!(
        as_planned(BUILD_IT, "pub fn build_it(depth: u32) -> u8 { 2 }").as_deref(),
        Some("pub fn build_it(\n    depth: u32,\n) -> u8 {\n    1\n}")
    );
}

#[test]
fn a_brace_in_a_string_or_a_comment_neither_opens_nor_closes_an_item() {
    let source = "pub enum E {\n    #[doc = \"}\"]\n    A, /* } */\n    B,\n}\n\nfn after() {}\n";
    assert_eq!(
        as_planned(source, "pub enum E { A, B, C }").as_deref(),
        Some("pub enum E {\n    #[doc = \"}\"]\n    A, /* } */\n    B,\n}")
    );
}

#[test]
fn an_impl_reads_only_the_members_the_plan_names() {
    let source = "impl Scope { pub fn root(self) -> u8 { 1 } pub fn other(&self) {} }";
    assert_eq!(
        as_planned(source, "impl Scope {\n    pub fn root(self) -> u8;\n}").as_deref(),
        Some("impl Scope {\n    pub fn root(self) -> u8;\n}")
    );
}

#[test]
fn a_member_is_read_from_whichever_block_of_its_impl_holds_it() {
    let source = "impl Scope {\n    pub fn new() -> Scope {\n        Scope\n    }\n}\n\nimpl Scope {\n    pub fn root(self) -> u8 {\n        1\n    }\n}\n";
    assert_eq!(
        as_planned(source, "impl Scope {\n    pub fn root(self) -> u16;\n}").as_deref(),
        Some("impl Scope {\n    pub fn root(self) -> u8;\n}")
    );
}

#[test]
fn each_planned_item_is_read_in_the_plan_s_order_a_blank_line_apart() {
    let source = "pub enum Theme { Light }\n\nimpl Settings {\n    pub fn theme(&self) -> Theme {\n        self.theme\n    }\n}\n";
    let planned =
        "pub enum Theme { Light, Dark }\n\nimpl Settings {\n    pub fn theme(&self) -> Theme;\n}\n";
    assert_eq!(
        as_planned(source, planned).as_deref(),
        Some("pub enum Theme { Light }\n\nimpl Settings {\n    pub fn theme(&self) -> Theme;\n}")
    );
}

#[test]
fn a_planned_function_the_top_does_not_hold_is_read_as_a_method() {
    let source = "pub struct Greeter;\n\nimpl Greeter {\n    pub fn hello(&self) -> String {\n        \"hello\".to_string()\n    }\n\n    pub fn bye(&self) -> String {\n        \"bye\".to_string()\n    }\n}\n";
    assert_eq!(
        as_planned(source, "pub fn bye(&self, name: &str) -> String;").as_deref(),
        Some("pub fn bye(&self) -> String;")
    );
}

#[test]
fn a_file_that_only_uses_a_name_holds_none_of_its_items() {
    assert_eq!(
        as_planned("use crate::other::Scope;\n", "pub struct Scope;"),
        None
    );
}

#[test]
fn an_item_quoted_in_a_raw_string_is_not_held() {
    let source = "const PLAN: &str = r#\"\npub fn build_it(depth: u32) {}\n\"#;\n";
    assert_eq!(as_planned(source, "pub fn build_it(depth: u32) {}"), None);
}

#[test]
fn each_signature_form_reads_back_exactly() {
    let forms = [
        (
            "pub(crate) async fn a(x: u8) -> Result<(), E> {}",
            "pub(crate) async fn a(x: u8) -> Result<(), E>;",
        ),
        (
            "pub fn b<T>(t: T) -> T\nwhere\n    T: Clone,\n{ t }",
            "pub fn b<T>(t: T) -> T\nwhere\n    T: Clone;",
        ),
        ("const fn c() -> u8 { 1 }", "const fn c() -> u8;"),
        ("extern \"C\" fn d(x: i32) {}", "extern \"C\" fn d(x: i32);"),
        (
            "pub fn e() -> (u8, u8) { (1, 2) }",
            "pub fn e() -> (u8, u8);",
        ),
        ("unsafe fn f(p: *const u8) {}", "unsafe fn f(p: *const u8);"),
        (
            "fn g<'a>(s: &'a str) -> &'a str { s }",
            "fn g<'a>(s: &'a str) -> &'a str;",
        ),
    ];
    for (source, signature) in forms {
        assert_eq!(
            as_planned(source, signature).as_deref(),
            Some(signature),
            "{source}"
        );
    }
}

#[test]
fn a_constant_reads_to_its_semicolon_whatever_block_its_value_holds() {
    let source = "const LIMIT: usize = if cfg!(test) { 1 } else { 2 };\nfn after() {}\n";
    assert_eq!(
        as_planned(source, "const LIMIT: usize = 3;").as_deref(),
        Some("const LIMIT: usize = if cfg!(test) { 1 } else { 2 };")
    );
}

#[test]
fn of_twins_the_first_is_read() {
    let source = "#[cfg(unix)]\nfn f() -> u8 { 1 }\n#[cfg(not(unix))]\nfn f() -> u16 { 2 }\n";
    assert_eq!(
        as_planned(source, "fn f();").as_deref(),
        Some("fn f() -> u8;")
    );
}

#[test]
fn an_item_the_top_does_not_hold_is_read_in_an_inline_module() {
    let source = "mod outer;\n\nmod inner {\n    pub struct S { a: u8 }\n}\n";
    assert_eq!(
        as_planned(source, "pub struct S { a: u8, b: u8 }").as_deref(),
        Some("pub struct S { a: u8 }")
    );
}

/// A `cfg` pair, a doc comment over its first twin.
const IDENTITY: &str = "/// What tells one file from another.\n#[cfg(unix)]\nfn identity(meta: &Metadata, _path: &Path) -> (u64, u64, PathBuf) {\n    (meta.dev(), meta.ino(), PathBuf::new())\n}\n\n#[cfg(not(unix))]\nfn identity(_meta: &Metadata, path: &Path) -> (u64, u64, PathBuf) {\n    (0, 0, path.to_path_buf())\n}\n";

#[test]
fn a_cfg_pair_planned_whole_reads_each_twin_once() {
    let planned = "#[cfg(unix)]\nfn identity(meta: &Metadata, _path: &Path) -> (u64, u64, PathBuf);\n\n#[cfg(not(unix))]\nfn identity(_meta: &Metadata, path: &Path) -> (u64, u64, PathBuf);";
    assert_eq!(as_planned(IDENTITY, planned).as_deref(), Some(planned));
}

#[test]
fn a_twin_planned_with_its_attributes_reads_the_twin_they_name() {
    let planned =
        "#[cfg(not(unix))]\nfn identity(_meta: &Metadata, path: &Path) -> (u64, u64, PathBuf);";
    assert_eq!(as_planned(IDENTITY, planned).as_deref(), Some(planned));
}

#[test]
fn twins_planned_without_attributes_are_read_in_turn() {
    let planned = "fn identity(meta: &Metadata, _path: &Path) -> (u64, u64, PathBuf);\n\nfn identity(_meta: &Metadata, path: &Path) -> (u64, u64, PathBuf);";
    assert_eq!(as_planned(IDENTITY, planned).as_deref(), Some(planned));
}

#[test]
fn a_method_s_cfg_pair_planned_whole_reads_each_twin_once() {
    let source = "impl Walk {\n    #[cfg(unix)]\n    fn id(&self) -> u64 {\n        1\n    }\n\n    #[cfg(not(unix))]\n    fn id(&self) -> u32 {\n        0\n    }\n}\n";
    let planned = "impl Walk {\n    #[cfg(unix)]\n    fn id(&self) -> u64;\n    #[cfg(not(unix))]\n    fn id(&self) -> u32;\n}";
    assert_eq!(as_planned(source, planned).as_deref(), Some(planned));
}

#[test]
fn one_block_today_is_read_for_each_block_the_plan_splits_it_into() {
    let source = "impl Scope {\n    pub fn a(&self) {}\n    pub fn b(&self) {}\n}\n";
    let planned = "impl Scope {\n    pub fn a(&self);\n}\n\nimpl Scope {\n    pub fn b(&self);\n}";
    assert_eq!(as_planned(source, planned).as_deref(), Some(planned));
}

#[test]
fn a_function_planned_with_a_receiver_reads_a_method_and_never_a_free_function() {
    let receivers = [
        "self",
        "mut self",
        "&self",
        "&mut self",
        "&'a self",
        "self: Box<Self>",
    ];
    for receiver in receivers {
        let method = format!("fn named<'a>({receiver}, name: &str) -> Option<&'a Named>");
        let source = format!(
            "impl Reading {{\n    {method} {{\n        None\n    }}\n}}\n\nfn named(shape: &Shape) -> Named {{\n    Named::Files\n}}\n"
        );
        let planned = format!("{method};");
        assert_eq!(
            as_planned(&source, &planned).as_deref(),
            Some(planned.as_str()),
            "{receiver}"
        );
        // One that takes no `self` reads the free function first.
        assert_eq!(
            as_planned(&source, "fn named(shape: &Shape, more: u8) -> Named;").as_deref(),
            Some("fn named(shape: &Shape) -> Named;")
        );
    }
}

/// A type's methods split across two blocks of different bounds.
const WRAPPER: &str = "impl<T> Wrapper<T> {\n    pub fn new(value: T) -> Self {\n        Wrapper(value)\n    }\n}\n\nimpl<T: Display> Wrapper<T> {\n    pub fn show(&self) -> String {\n        self.0.to_string()\n    }\n}\n";

#[test]
fn a_member_is_read_under_the_header_of_the_block_that_holds_it() {
    let bounded = "impl<T: Display> Wrapper<T> {\n    pub fn show(&self) -> String;\n}";
    assert_eq!(as_planned(WRAPPER, bounded).as_deref(), Some(bounded));
    // A plan that takes it out of its block reads the bound it drops.
    let moved = "impl<T> Wrapper<T> {\n    pub fn show(&self) -> String;\n}";
    assert_eq!(as_planned(WRAPPER, moved).as_deref(), Some(bounded));
    let bound_by_where = WRAPPER.replace(
        "impl<T: Display> Wrapper<T> {",
        "impl<T> Wrapper<T>\nwhere\n    T: Display,\n{",
    );
    let planned =
        "impl<T> Wrapper<T>\nwhere\n    T: Display,\n{\n    pub fn show(&self) -> String;\n}";
    assert_eq!(
        as_planned(&bound_by_where, planned).as_deref(),
        Some(planned)
    );
}

#[test]
fn members_of_two_blocks_are_read_under_the_plan_s_header_where_one_has_it() {
    let planned = "impl<T: Display> Wrapper<T> {\n    pub fn new(value: T) -> Self;\n    pub fn show(&self) -> String;\n}";
    assert_eq!(as_planned(WRAPPER, planned).as_deref(), Some(planned));
}

#[test]
fn a_brace_in_a_const_argument_opens_no_body() {
    let source = "pub fn buffer() -> ArrayVec<u8, { limits::CAP }> {\n    ArrayVec::new()\n}\n\npub fn other() -> u8 {\n    1\n}\n\nimpl Buf<{ N }> {\n    pub fn len(&self) -> usize {\n        N\n    }\n}\n";
    for planned in [
        "pub fn buffer() -> ArrayVec<u8, { limits::CAP }>;",
        "pub fn other() -> u8;",
        "impl Buf<{ N }> {\n    pub fn len(&self) -> usize;\n}",
    ] {
        assert_eq!(
            as_planned(source, planned).as_deref(),
            Some(planned),
            "{planned}"
        );
    }
}

/// In `<'a>(&'a self` a lifetime's quote stands four bytes from the next:
/// it opens no char, and the method still ends at its body.
#[test]
fn a_lifetime_opens_no_char_however_near_the_next_quote_stands() {
    let source = "impl Config {\n    pub fn role<'a>(&'a self, node: &'a Node) -> Option<&'a str> {\n        None\n    }\n\n    pub fn other(&self) -> u8 {\n        1\n    }\n}\n";
    for planned in [
        "pub fn role<'a>(&'a self, node: &'a Node) -> Option<&'a str>;",
        "pub fn other(&self) -> u8;",
    ] {
        assert_eq!(
            as_planned(source, planned).as_deref(),
            Some(planned),
            "{planned}"
        );
    }
}

#[test]
fn a_test_module_s_helper_is_not_what_the_plan_adds() {
    let source = "pub fn names_in_code(source: &str) -> bool {\n    true\n}\n\n#[cfg(test)]\nmod tests {\n    fn read(kind: &str, sources: &[&str]) -> Vec<bool> {\n        Vec::new()\n    }\n}\n";
    assert_eq!(
        as_planned(source, "pub(crate) fn read(source: &str) -> Reading;"),
        None
    );
}

#[test]
fn a_trait_reads_only_the_members_the_plan_names() {
    let source = "pub trait Store {\n    fn load(&self) -> u8;\n\n    fn save(&mut self, value: u8) {\n        let _ = value;\n    }\n}\n";
    assert_eq!(
        as_planned(source, "pub trait Store {\n    fn load(&self) -> u16;\n}").as_deref(),
        Some("pub trait Store {\n    fn load(&self) -> u8;\n}")
    );
}

#[test]
fn a_generic_impl_is_known_by_its_head_past_its_generics_and_before_its_where() {
    let source = "impl<T: Clone> Scope<T> where T: Send { pub fn root(&self) {} }";
    assert_eq!(
        as_planned(
            source,
            "impl<T: Clone> Scope<T> {\n    pub fn root(&self);\n}"
        )
        .as_deref(),
        Some("impl<T: Clone> Scope<T> where T: Send {\n    pub fn root(&self);\n}")
    );
}

#[test]
fn the_arrow_of_a_function_type_in_an_impl_s_generics_closes_nothing() {
    let source =
        "impl<F: Fn() -> u8> Runner<F> {\n    pub fn run(&self) -> u8 {\n        (self.0)()\n    }\n}\n";
    assert_eq!(
        as_planned(
            source,
            "impl<F: Fn() -> u8> Runner<F> {\n    pub fn run(&self) -> u16;\n}"
        )
        .as_deref(),
        Some("impl<F: Fn() -> u8> Runner<F> {\n    pub fn run(&self) -> u8;\n}")
    );
}
