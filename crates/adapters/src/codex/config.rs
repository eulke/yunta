//! `codex exec -c key=value`: the CLI's own way to set one config key
//! for a single invocation. The value is TOML, so a path or a URL that
//! carries a `"` or a `\` ends the literal it sits in and turns the
//! rest of the value into syntax the CLI either rejects or, worse,
//! reads as another key. [`ConfigOverride`] is the only way this
//! adapter builds an override, and it renders every value through the
//! `toml` crate — which picks the quoting a value needs, basic or
//! literal — so an unquoted one is unrepresentable here.

use toml::Value;

/// One `-c key=value` override, with its value already rendered as
/// TOML. Rendering goes through [`ConfigOverride::into_args`], which
/// emits the flag together with its assignment, so a caller cannot hand
/// the CLI an assignment with no `-c` in front of it.
pub(super) struct ConfigOverride {
    key: String,
    value: Value,
}

impl ConfigOverride {
    /// A TOML boolean: bare `true` or `false`, never quoted — the CLI
    /// reads a quoted one as a string and rejects it for a key typed as
    /// a boolean.
    pub(super) fn boolean(key: impl Into<String>, value: bool) -> Self {
        Self {
            key: key.into(),
            value: Value::Boolean(value),
        }
    }

    /// A TOML string.
    pub(super) fn string(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: Value::String(value.into()),
        }
    }

    /// A TOML array of strings.
    pub(super) fn list<I>(key: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self {
            key: key.into(),
            value: Value::Array(
                values
                    .into_iter()
                    .map(|v| Value::String(v.into()))
                    .collect(),
            ),
        }
    }

    /// The two `argv` entries the CLI takes for this override.
    pub(super) fn into_args(self) -> [String; 2] {
        ["-c".to_string(), format!("{}={}", self.key, self.value)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The assignment half of an override, with the flag asserted.
    fn assignment(one: ConfigOverride) -> String {
        let [flag, assignment] = one.into_args();
        assert_eq!(flag, "-c", "the flag travels with its assignment");
        assignment
    }

    /// What the CLI reads back from an assignment under the key `k` —
    /// the only claim that matters, since which quoting a value takes
    /// is the renderer's call and not this adapter's.
    fn read_back(one: ConfigOverride) -> Value {
        let assignment = assignment(one);
        let table: toml::Table = toml::from_str(&assignment)
            .unwrap_or_else(|e| panic!("`{assignment}` is not readable TOML: {e}"));
        table["k"].clone()
    }

    #[test]
    fn a_boolean_renders_unquoted() {
        assert_eq!(assignment(ConfigOverride::boolean("k", true)), "k=true");
    }

    #[test]
    fn a_plain_string_survives_the_round_trip() {
        let url = "http://127.0.0.1:54321/mcp";
        assert_eq!(
            assignment(ConfigOverride::string("k", url)),
            format!("k=\"{url}\"")
        );
        assert_eq!(
            read_back(ConfigOverride::string("k", url)),
            Value::String(url.to_string())
        );
    }

    #[test]
    fn a_path_carrying_a_quote_and_a_backslash_reads_back_whole() {
        // The defect this type exists for: interpolated raw, this path
        // closes the literal it sits in and leaves the rest of itself
        // to be read as syntax.
        let path = r#"/tmp/quote"and\slash"#;
        assert_eq!(
            read_back(ConfigOverride::string("k", path)),
            Value::String(path.to_string())
        );
    }

    #[test]
    fn whitespace_and_control_characters_read_back_whole() {
        for value in ["a\nb", "a\tb", "a\u{1}b", "a\u{7f}b", "a\rb"] {
            assert_eq!(
                read_back(ConfigOverride::string("k", value)),
                Value::String(value.to_string()),
                "{value:?} does not survive the round trip"
            );
        }
    }

    #[test]
    fn text_outside_ascii_reads_back_whole() {
        assert_eq!(
            read_back(ConfigOverride::string("k", "ñandú")),
            Value::String("ñandú".to_string())
        );
    }

    #[test]
    fn a_list_reads_back_as_its_items() {
        let items = ["/run/artifacts".to_string(), r#"/odd"path"#.to_string()];
        assert_eq!(
            read_back(ConfigOverride::list("k", items.clone())),
            Value::Array(items.into_iter().map(Value::String).collect())
        );
    }

    #[test]
    fn an_empty_list_renders_as_an_empty_array() {
        assert_eq!(
            assignment(ConfigOverride::list("k", Vec::<String>::new())),
            "k=[]"
        );
    }
}
