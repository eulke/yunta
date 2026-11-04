use super::{Kind, Text};
use crate::code::Language;

/// The items `source`, code in `language`, declares at its top, each as its
/// kind, its name and its text from its head to its end.
fn read(source: &str, language: Language) -> Vec<(Kind, String, &str)> {
    let text = Text::of(source, language).expect("its items are read");
    text.items()
        .into_iter()
        .map(|item| (item.kind, item.name, &source[item.head..item.bounds.end]))
        .collect()
}

/// The members the first item `source` declares holds, each as its kind,
/// its name and its text from its head to its end.
fn members(source: &str, language: Language) -> Vec<(Kind, String, &str)> {
    let text = Text::of(source, language).expect("its items are read");
    let items = text.items();
    let holder = items.first().expect("an item holds the members");
    text.inside(holder)
        .into_iter()
        .map(|item| (item.kind, item.name, &source[item.head..item.bounds.end]))
        .collect()
}

/// Rust's items are read by their `;` and their braces, and a language
/// whose items are not read has no text to read them in.
#[test]
fn rust_reads_by_its_braces_and_another_language_reads_none() {
    assert_eq!(
        read("fn a() {}\nstruct B;\n", Language::Rust),
        [
            (Kind::Fn, "a".to_string(), "fn a() {}"),
            (Kind::Struct, "B".to_string(), "struct B;"),
        ]
    );
    assert!(Text::of("a: 1\n", Language::Yaml).is_none());
}

/// Go declares functions, methods by their receiver's type, types, and
/// constants and variables alone or in groups; a `package` and an `import`
/// declare none.
#[test]
fn go_declares_its_functions_methods_types_and_groups() {
    let source = "package store\n\nimport \"fmt\"\n\nconst (\n\tSmall = 1\n\tLarge = 2\n)\n\ntype Store struct {\n\titems []string\n}\n\nfunc (s *Store) Load(name string) error {\n\treturn fmt.Errorf(\"%s\", name)\n}\n\nfunc New() *Store { return &Store{} }\n\nvar Default = New()\n";
    assert_eq!(
        read(source, Language::Go),
        [
            (Kind::Const, "Small".to_string(), "Small = 1"),
            (Kind::Const, "Large".to_string(), "Large = 2"),
            (
                Kind::Type,
                "Store".to_string(),
                "type Store struct {\n\titems []string\n}"
            ),
            (
                Kind::Fn,
                "Store.Load".to_string(),
                "func (s *Store) Load(name string) error {\n\treturn fmt.Errorf(\"%s\", name)\n}"
            ),
            (
                Kind::Fn,
                "New".to_string(),
                "func New() *Store { return &Store{} }"
            ),
            (Kind::Var, "Default".to_string(), "var Default = New()"),
        ]
    );
}

/// A script declares functions, the names it binds to functions, classes,
/// interfaces, types and variables, past `export` and `default`; an import
/// declares none, and a statement with no `;` ends where its lines do.
#[test]
fn a_script_declares_its_functions_classes_and_bindings() {
    let source = "import { x } from \"./x\";\n\nexport interface Shape {\n  area(): number;\n}\n\nexport const unit = (r: number): Circle => {\n  return new Circle(r);\n};\n\nexport default function draw(shape: Shape) {\n  console.log(shape.area());\n}\n\ntype Id = string;\nlet count = 0\nconst limit = 10\n";
    assert_eq!(
        read(source, Language::Script),
        [
            (
                Kind::Interface,
                "Shape".to_string(),
                "export interface Shape {\n  area(): number;\n}"
            ),
            (
                Kind::Fn,
                "unit".to_string(),
                "export const unit = (r: number): Circle => {\n  return new Circle(r);\n};"
            ),
            (
                Kind::Fn,
                "draw".to_string(),
                "export default function draw(shape: Shape) {\n  console.log(shape.area());\n}"
            ),
            (Kind::Type, "Id".to_string(), "type Id = string;"),
            (Kind::Var, "count".to_string(), "let count = 0"),
            (Kind::Const, "limit".to_string(), "const limit = 10"),
        ]
    );
}

/// A script's class holds its methods, its constructor and an accessor
/// among them, and its fields that hold a function; a field that holds a
/// value declares nothing.
#[test]
fn a_script_class_holds_its_methods() {
    let source = "export class Circle implements Shape {\n  private r = 1;\n  static readonly unit = 1;\n\n  constructor(r: number) {\n    this.r = r;\n  }\n\n  get area(): number {\n    return Math.PI * this.r ** 2;\n  }\n\n  scale = (by: number) => new Circle(this.r * by);\n}\n";
    assert_eq!(
        members(source, Language::Script),
        [
            (
                Kind::Fn,
                "constructor".to_string(),
                "constructor(r: number) {\n    this.r = r;\n  }"
            ),
            (
                Kind::Fn,
                "area".to_string(),
                "get area(): number {\n    return Math.PI * this.r ** 2;\n  }"
            ),
            (
                Kind::Fn,
                "scale".to_string(),
                "scale = (by: number) => new Circle(this.r * by);"
            ),
        ]
    );
}

/// Python declares its functions and classes by their lines, with the
/// decorators above them; a class holds its methods, and a method takes a
/// receiver where its first parameter is `self` or `cls`.
#[test]
fn python_declares_by_its_lines_and_a_class_holds_its_methods() {
    let source = "import os\n\n\n@dataclass\nclass Store:\n    root: str\n\n    def load(self, name):\n        path = os.path.join(self.root, name)\n        return open(path).read()\n\n    @classmethod\n    def empty(cls):\n        return cls(\"\")\n\n\nasync def fetch(url):\n    return await get(url)\n";
    let text = Text::of(source, Language::Python).expect("Python's items are read");
    let items = text.items();
    let read: Vec<(Kind, &str, &str)> = items
        .iter()
        .map(|item| {
            (
                item.kind,
                item.name.as_str(),
                &source[item.start..item.bounds.end],
            )
        })
        .collect();
    assert_eq!(
        read,
        [
            (
                Kind::Class,
                "Store",
                "@dataclass\nclass Store:\n    root: str\n\n    def load(self, name):\n        path = os.path.join(self.root, name)\n        return open(path).read()\n\n    @classmethod\n    def empty(cls):\n        return cls(\"\")"
            ),
            (Kind::Fn, "fetch", "async def fetch(url):\n    return await get(url)"),
        ]
    );
    let methods: Vec<(String, bool, &str)> = text
        .inside(&items[0])
        .into_iter()
        .map(|item| {
            (
                item.name,
                item.receiver,
                &source[item.start..item.bounds.end],
            )
        })
        .collect();
    assert_eq!(
        methods,
        [
            (
                "load".to_string(),
                true,
                "def load(self, name):\n        path = os.path.join(self.root, name)\n        return open(path).read()"
            ),
            (
                "empty".to_string(),
                true,
                "@classmethod\n    def empty(cls):\n        return cls(\"\")"
            ),
        ]
    );
}

/// A Python head stops at the `:` outside its brackets, however many lines
/// its parameters take, and its body is what follows.
#[test]
fn a_python_head_stops_at_its_colon() {
    let source = "def f(a: int,\n      b: str) -> Dict[str, int]:\n    return {}\n";
    let text = Text::of(source, Language::Python).expect("Python's items are read");
    let items = text.items();
    let item = items.first().expect("the function is read");
    assert_eq!(
        &source[item.head..item.bounds.stop],
        "def f(a: int,\n      b: str) -> Dict[str, int]"
    );
    assert_eq!(
        item.bounds.body.clone().map(|body| &source[body]),
        Some("\n    return {}")
    );
}

/// A brace in a string or a comment neither opens nor closes an item, and
/// an item ends where its lines do though it writes no `;`.
#[test]
fn a_brace_in_a_string_opens_nothing_and_an_item_ends_with_its_lines() {
    let go = "func a() string {\n\treturn \"}\" // }\n}\nfunc b() {}\n";
    assert_eq!(
        read(go, Language::Go),
        [
            (
                Kind::Fn,
                "a".to_string(),
                "func a() string {\n\treturn \"}\" // }\n}"
            ),
            (Kind::Fn, "b".to_string(), "func b() {}"),
        ]
    );
    let script = "const a = 1\nfunction b() {}\n";
    assert_eq!(
        read(script, Language::Script),
        [
            (Kind::Const, "a".to_string(), "const a = 1"),
            (Kind::Fn, "b".to_string(), "function b() {}"),
        ]
    );
}

/// A regular expression's bracket opens nothing, so the statement after it
/// is read as its own.
#[test]
fn a_regular_expression_s_bracket_opens_nothing() {
    assert_eq!(
        read(
            "const open = /\\(/;\nfunction helper() {}\n",
            Language::Script
        ),
        [
            (Kind::Const, "open".to_string(), "const open = /\\(/;"),
            (Kind::Fn, "helper".to_string(), "function helper() {}"),
        ]
    );
}

/// A decorator on a member's own line decorates that member, and the member
/// after it starts at its own words.
#[test]
fn a_decorator_on_a_member_s_line_decorates_it() {
    let source = "export class Panel {\n  @Input() name: string;\n  @HostListener('click') onClick() {\n    this.open();\n  }\n  close() {}\n}\n";
    assert_eq!(
        members(source, Language::Script),
        [
            (
                Kind::Fn,
                "onClick".to_string(),
                "onClick() {\n    this.open();\n  }"
            ),
            (Kind::Fn, "close".to_string(), "close() {}"),
        ]
    );
}

/// A Python head with nothing after its colon, as a plan writes a
/// signature, stops at the colon and has no body.
#[test]
fn a_python_head_with_nothing_after_its_colon_has_no_body() {
    let source = "def load(name):";
    let text = Text::of(source, Language::Python).expect("Python's items are read");
    let items = text.items();
    let item = items.first().expect("the function is read");
    assert_eq!(&source[item.head..item.bounds.stop], "def load(name)");
    assert!(item.bounds.body.is_none());
}

/// A script's arrow whose body is an expression has that expression for its
/// body, after its `=>`.
#[test]
fn a_script_arrow_s_expression_is_its_body() {
    let source = "const f = (x) => x + 1;";
    let text = Text::of(source, Language::Script).expect("a script's items are read");
    let items = text.items();
    let item = items.first().expect("the function is read");
    assert_eq!(item.kind, Kind::Fn);
    assert_eq!(&source[item.bounds.stop..item.bounds.stop + 2], "=>");
    assert_eq!(
        item.bounds.body.clone().map(|body| &source[body]),
        Some(" x + 1")
    );
}

/// Each item of a Go group carries the keyword the group writes once.
#[test]
fn a_go_group_s_items_carry_its_keyword() {
    let source = "var (\n\tA = 1\n\tB = 2\n)\n";
    let text = Text::of(source, Language::Go).expect("Go's items are read");
    let leads: Vec<Option<&str>> = text
        .items()
        .iter()
        .map(|item| item.lead.clone().map(|lead| &source[lead]))
        .collect();
    assert_eq!(leads, [Some("var"), Some("var")]);
}

/// A name past ASCII is read whole, and a byte-order mark hides no
/// declaration.
#[test]
fn a_name_past_ascii_is_whole_and_a_byte_order_mark_hides_nothing() {
    assert_eq!(
        read(
            "\u{feff}def café():\n    pass\ndef caf():\n    pass\n",
            Language::Python
        ),
        [
            (Kind::Fn, "café".to_string(), "def café():\n    pass"),
            (Kind::Fn, "caf".to_string(), "def caf():\n    pass"),
        ]
    );
}

/// A decorator whose line ends in a carriage return and a line feed starts
/// the item below it, as one whose line ends in a line feed does.
#[test]
fn a_decorator_above_a_crlf_line_starts_its_item() {
    let source = "@dataclass\r\nclass Store:\r\n    pass\r\n";
    let text = Text::of(source, Language::Python).expect("Python's items are read");
    let items = text.items();
    let item = items.first().expect("the class is read");
    assert_eq!(item.kind, Kind::Class);
    assert_eq!(
        &source[item.start..item.bounds.end],
        "@dataclass\r\nclass Store:\r\n    pass"
    );
}

/// A decorator above a member, on a line that ends in a carriage return and a
/// line feed, starts the member's item.
#[test]
fn a_decorator_above_a_crlf_member_starts_its_member() {
    let source = "export class Panel {\r\n  @HostListener('click')\r\n  onClick() {\r\n    this.open();\r\n  }\r\n}\r\n";
    let text = Text::of(source, Language::Script).expect("a script's items are read");
    let items = text.items();
    let holder = items.first().expect("the class is read");
    let members = text.inside(holder);
    let member = members.first().expect("the method is read");
    assert_eq!(member.name, "onClick");
    assert_eq!(
        &source[member.start..member.bounds.end],
        "@HostListener('click')\r\n  onClick() {\r\n    this.open();\r\n  }"
    );
}

/// A byte-order mark at a file's start is read as blank and is no indent, so
/// the body of a Python head indented two columns is read.
#[test]
fn a_byte_order_mark_is_no_indent() {
    let source = "\u{feff}def f():\n  pass\n";
    let text = Text::of(source, Language::Python).expect("Python's items are read");
    let items = text.items();
    let item = items.first().expect("the function is read");
    assert_eq!(
        item.bounds.body.clone().map(|body| &source[body]),
        Some("\n  pass")
    );
}
