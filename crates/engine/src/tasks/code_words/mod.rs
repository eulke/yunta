//! Whether a file names a word as code: outside its literals and its
//! comments, or inside what a literal interpolates. A test fixture that
//! quotes a plan, or a doc comment that mentions a shape, says the name
//! and calls nothing; `${store.load()}` in a template calls it.
//!
//! The reading walks code, and each literal and comment it meets in the
//! way its language writes them — see [`syntax`] — then walks back as
//! code every hole a literal holds. A file of a type it does not know is
//! read whole, so an unknown language never loses a real caller.

mod syntax;

use syntax::{is_word, Hole, Lang, Literal};

/// Whether `source`, a file whose extension is `kind`, names `word` as
/// code.
pub(crate) fn names_in_code(source: &str, kind: &str, word: &str) -> bool {
    let code = match Lang::of(kind) {
        Some(lang) => blanked(source.as_bytes(), lang),
        None => source.as_bytes().to_vec(),
    };
    contains_word(&code, word.as_bytes())
}

/// `source` with what is not code turned to spaces — newlines kept — so
/// what is left is the code, word boundaries intact.
fn blanked(source: &[u8], lang: Lang) -> Vec<u8> {
    let mut walk = Walk {
        source,
        lang,
        out: source.to_vec(),
    };
    walk.code(0, None);
    walk.out
}

/// One file's walk, and what it has blanked so far.
struct Walk<'a> {
    source: &'a [u8],
    lang: Lang,
    out: Vec<u8>,
}

impl Walk<'_> {
    /// Walks code from `at` to the end — or, in a literal's hole, past
    /// the `close` that ends it — and answers where it stopped.
    fn code(&mut self, mut at: usize, close: Option<u8>) -> usize {
        let open = close.map(|close| match close {
            b')' => b'(',
            _ => b'{',
        });
        let mut depth = 0usize;
        while let Some(&byte) = self.source.get(at) {
            if let Some(end) = self.lang.comment(self.source, at) {
                self.blank(at, end);
                at = end;
                continue;
            }
            if let Some(literal) = self.lang.literal(self.source, at) {
                at = self.literal(at, literal);
                continue;
            }
            if Some(byte) == close && depth == 0 {
                return at + 1;
            }
            if Some(byte) == open {
                depth += 1;
            } else if Some(byte) == close {
                depth -= 1;
            }
            at += 1;
        }
        at
    }

    /// Blanks the literal that opens at `at`, but for the holes in its
    /// text, which it walks as code, and answers where it ends.
    fn literal(&mut self, at: usize, literal: Literal) -> usize {
        let mut text = at;
        let mut index = literal.text;
        while index < self.source.len() {
            match literal.holes.at(self.source, index) {
                Some(Hole::Code { from, close }) => {
                    self.blank(text, from);
                    index = self.code(from, Some(close));
                    text = index;
                    continue;
                }
                Some(Hole::Name { from, to }) => {
                    self.blank(text, from);
                    index = to;
                    text = index;
                    continue;
                }
                Some(Hole::Text { to }) => {
                    index = to;
                    continue;
                }
                None => {}
            }
            if syntax::bytes_from(self.source, index).starts_with(&literal.close) {
                let end = index + literal.close.len();
                self.blank(text, end);
                return end;
            }
            index += match literal.escapes && self.source.get(index) == Some(&b'\\') {
                true => 2,
                false => 1,
            };
        }
        self.blank(text, self.source.len());
        self.source.len()
    }

    fn blank(&mut self, from: usize, to: usize) {
        let to = to.min(self.out.len());
        for byte in self.out.iter_mut().take(to).skip(from) {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
}

/// Whether `word` stands whole somewhere in `code`.
fn contains_word(code: &[u8], word: &[u8]) -> bool {
    if word.is_empty() {
        return false;
    }
    code.windows(word.len())
        .enumerate()
        .filter(|(_, window)| *window == word)
        .any(|(at, _)| {
            let before = at.checked_sub(1).and_then(|i| code.get(i)).copied();
            let after = code.get(at + word.len()).copied();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
}

#[cfg(test)]
mod tests {
    use super::names_in_code;

    /// Whether each of `sources`, in a file of `kind`, names `PackStore`
    /// as code.
    fn read(kind: &str, sources: &[&str]) -> Vec<bool> {
        sources
            .iter()
            .map(|source| names_in_code(source, kind, "PackStore"))
            .collect()
    }

    #[test]
    fn a_name_in_a_fixture_string_calls_nothing() {
        let fixture = "let plan = r##\"\n  - name: PackStore\n    code: PackStore::new()\n\"##;\n";
        let quoted = "let at = \"src/pack.rs::PackStore\"; // PackStore\n";
        let commented = "/* PackStore */ fn f() {}";
        assert_eq!(read("rs", &[fixture, quoted, commented]), [false; 3]);
    }

    #[test]
    fn a_name_in_code_is_a_caller() {
        let called = "let store = PackStore::new();";
        let lifetime = "fn f<'a>(s: &'a PackStore) -> char { 'x' }";
        assert_eq!(read("rs", &[called, lifetime]), [true; 2]);
        assert_eq!(read("ts", &["const s = `${a}`; new PackStore()"]), [true]);
    }

    #[test]
    fn python_hides_its_own_strings_and_comments() {
        let hidden = "# PackStore\nx = '''PackStore'''\n";
        let called = "x = PackStore()  # the store\n";
        assert_eq!(read("py", &[hidden, called]), [false, true]);
    }

    #[test]
    fn a_language_it_does_not_know_is_read_whole() {
        assert!(names_in_code("store: \"PackStore\"\n", "yaml", "PackStore"));
        assert!(!names_in_code("PackStores", "yaml", "PackStore"));
    }

    #[test]
    fn a_nested_comment_hides_its_whole_body() {
        let nested = "/* a /* b */ PackStore */ fn f() {}";
        assert_eq!(read("rs", &[nested]), [false]);
        assert_eq!(read("swift", &[nested]), [false]);
        // Where comments do not nest, the first `*/` ends the comment.
        assert_eq!(read("c", &["/* a /* b */ PackStore();"]), [true]);
    }

    #[test]
    fn a_template_interpolation_is_code() {
        let sources = [
            "const s = `made by ${PackStore.name()}`;",
            "const s = `${`inner ${PackStore}`}`;",
            "const s = `${\"}\"} PackStore`;",
            "const s = `PackStore`;",
            "const s = '${PackStore}';",
        ];
        assert_eq!(read("ts", &sources), [true, true, false, false, false]);
    }

    #[test]
    fn an_fstring_field_is_code() {
        let sources = [
            "s = f\"{PackStore()}\"",
            "s = rf'{PackStore}'",
            "s = f\"{width:{PackStore}}\"",
            "s = f\"{{PackStore}}\"",
            "s = \"{PackStore}\"",
            "s = f'''\n{x}\n''' + 'PackStore'",
        ];
        assert_eq!(
            read("py", &sources),
            [true, true, true, false, false, false]
        );
    }

    #[test]
    fn each_language_writes_code_into_its_strings_its_own_way() {
        let kotlin = ["\"${PackStore()}\"", "\"$PackStore\"", "\"\\$PackStore\""];
        assert_eq!(read("kt", &kotlin), [true, true, false]);
        let swift = [
            "\"\\(PackStore())\"",
            "#\"\\(PackStore)\"#",
            "#\"\\#(PackStore)\"#",
        ];
        assert_eq!(read("swift", &swift), [true, false, true]);
        let csharp = [
            "$\"{PackStore}\"",
            "$\"{{PackStore}}\"",
            "@\"PackStore\"",
            "$$\"\"\"{{PackStore}}\"\"\"",
        ];
        assert_eq!(read("cs", &csharp), [true, false, false, true]);
        let dart = ["'${PackStore()}'", "'$PackStore'", "r'$PackStore'"];
        assert_eq!(read("dart", &dart), [true, true, false]);
        let scala = ["s\"$PackStore\"", "s\"${PackStore()}\"", "\"$PackStore\""];
        assert_eq!(read("scala", &scala), [true, true, false]);
    }

    #[test]
    fn a_raw_string_ends_only_where_it_says() {
        let rust = "let s = r#\"a \"quote\" PackStore\"#; f()";
        assert_eq!(read("rs", &[rust]), [false]);
        let cpp = "auto s = R\"x(a\"b PackStore)x\"; f();";
        assert_eq!(read("cpp", &[cpp]), [false]);
        let go = ["s := `PackStore` + f()", "s := `a \\` + PackStore()"];
        assert_eq!(read("go", &go), [false, true]);
    }
}
