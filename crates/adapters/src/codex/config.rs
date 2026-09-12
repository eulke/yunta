//! `codex exec -c key=value`: the CLI's own way to set one config key
//! for a single invocation. The value is TOML, so a path or a URL that
//! carries a `"` or a `\` ends the literal it sits in and turns the
//! rest of the value into syntax the CLI either rejects or, worse,
//! reads as another key. [`ConfigOverride`] is the only way this
//! adapter builds an override, and every constructor that takes text
//! renders it as an escaped TOML basic string, so an unescaped value
//! is unrepresentable here.

/// One `-c key=value` override, with its value already rendered as
/// TOML. Rendering goes through [`ConfigOverride::into_args`], which
/// emits the flag together with its assignment, so a caller cannot
/// hand the CLI an assignment with no `-c` in front of it.
pub(super) struct ConfigOverride {
    key: String,
    value: String,
}

impl ConfigOverride {
    /// A TOML boolean: bare `true` or `false`, never quoted — the CLI
    /// reads a quoted one as a string and rejects it for a key typed
    /// as a boolean.
    pub(super) fn boolean(key: impl Into<String>, value: bool) -> Self {
        Self {
            key: key.into(),
            value: value.to_string(),
        }
    }

    /// A TOML basic string, escaped.
    pub(super) fn string(key: impl Into<String>, value: impl AsRef<str>) -> Self {
        Self {
            key: key.into(),
            value: basic_string(value.as_ref()),
        }
    }

    /// A TOML array of basic strings, each one escaped.
    pub(super) fn list<I>(key: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let items: Vec<String> = values
            .into_iter()
            .map(|value| basic_string(value.as_ref()))
            .collect();
        Self {
            key: key.into(),
            value: format!("[{}]", items.join(", ")),
        }
    }

    /// The two argv entries the CLI expects: the `-c` flag and the
    /// `key=value` it applies to.
    pub(super) fn into_args(self) -> [String; 2] {
        ["-c".to_string(), format!("{}={}", self.key, self.value)]
    }
}

/// `value` as a TOML basic string, quotes included.
///
/// TOML gives a basic string five named escapes (`\b`, `\t`, `\n`,
/// `\f`, `\r`) and spells every other control character — U+0000
/// through U+001F, plus the delete at U+007F — as `\uXXXX`. Anything
/// else is legal inside the quotes as itself, UTF-8 included, so it
/// passes through untouched.
fn basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str("\\\""),
            '\u{8}' => out.push_str(r"\b"),
            '\t' => out.push_str(r"\t"),
            '\n' => out.push_str(r"\n"),
            '\u{c}' => out.push_str(r"\f"),
            '\r' => out.push_str(r"\r"),
            control if (control as u32) < 0x20 || control == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `key=value` half of the rendering, for the tests that read
    /// the TOML rather than the argv shape.
    fn assignment(setting: ConfigOverride) -> String {
        let [_, assignment] = setting.into_args();
        assignment
    }

    #[test]
    fn an_override_renders_as_the_flag_and_its_assignment() {
        assert_eq!(
            ConfigOverride::boolean("experimental_use_rmcp_client", true).into_args(),
            [
                "-c".to_string(),
                "experimental_use_rmcp_client=true".to_string()
            ]
        );
    }

    #[test]
    fn a_boolean_renders_unquoted() {
        assert_eq!(
            assignment(ConfigOverride::boolean("flag", false)),
            "flag=false"
        );
    }

    #[test]
    fn a_plain_string_renders_unchanged_inside_quotes() {
        assert_eq!(
            assignment(ConfigOverride::string(
                "mcp_servers.yunta.url",
                "http://127.0.0.1:54321/mcp"
            )),
            r#"mcp_servers.yunta.url="http://127.0.0.1:54321/mcp""#
        );
    }

    #[test]
    fn a_list_renders_as_a_toml_array() {
        assert_eq!(
            assignment(ConfigOverride::list(
                "sandbox_workspace_write.writable_roots",
                ["/run/artifacts", "/run/scratch"]
            )),
            r#"sandbox_workspace_write.writable_roots=["/run/artifacts", "/run/scratch"]"#
        );
    }

    #[test]
    fn an_empty_list_renders_as_an_empty_array() {
        assert_eq!(
            assignment(ConfigOverride::list("roots", Vec::<String>::new())),
            "roots=[]"
        );
    }

    #[test]
    fn a_double_quote_is_escaped() {
        assert_eq!(
            assignment(ConfigOverride::string("key", r#"a "quoted" name"#)),
            r#"key="a \"quoted\" name""#
        );
    }

    #[test]
    fn a_backslash_is_escaped() {
        assert_eq!(
            assignment(ConfigOverride::string("key", r"C:\runs\artifacts")),
            r#"key="C:\\runs\\artifacts""#
        );
    }

    #[test]
    fn a_newline_and_a_tab_render_as_their_named_escapes() {
        assert_eq!(
            assignment(ConfigOverride::string("key", "one\ntwo\tthree")),
            r#"key="one\ntwo\tthree""#
        );
    }

    #[test]
    fn a_backspace_a_form_feed_and_a_carriage_return_render_as_their_named_escapes() {
        assert_eq!(
            assignment(ConfigOverride::string("key", "a\u{8}b\u{c}c\r")),
            r#"key="a\bb\fc\r""#
        );
    }

    #[test]
    fn any_other_control_character_renders_as_its_unicode_escape() {
        assert_eq!(
            assignment(ConfigOverride::string("key", "a\u{0}b\u{1f}c\u{7f}")),
            r#"key="a\u0000b\u001Fc\u007F""#
        );
    }

    #[test]
    fn text_outside_ascii_passes_through_as_itself() {
        assert_eq!(
            assignment(ConfigOverride::string("key", "/runs/año/día")),
            r#"key="/runs/año/día""#
        );
    }

    #[test]
    fn an_escaped_item_is_escaped_inside_a_list_too() {
        assert_eq!(
            assignment(ConfigOverride::list("roots", [r#"/a"b\c"#])),
            r#"roots=["/a\"b\\c"]"#
        );
    }
}
