//! The small ANSI palette terminal messages may use, and the policy
//! that decides whether a particular stream may carry it.

/// The ANSI SGR colors used for messages. These are members of the
/// standard 16-color palette; no terminal-specific RGB color is needed
/// to tell one kind of message from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorRole {
    /// A command could not complete.
    Error,
    /// Something needs attention, though the command can continue.
    Warning,
    /// A finding or other useful information.
    Info,
}

impl ColorRole {
    const fn sgr(self) -> &'static str {
        match self {
            Self::Error => "1;31",
            Self::Warning => "1;33",
            Self::Info => "1;36",
        }
    }
}

/// Whether a stream can display ANSI color.
///
/// The caller supplies the terminal status of the stream that will
/// receive the text, along with `NO_COLOR` as it was present in the
/// environment. Presence disables color, including an empty value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ColorPolicy {
    enabled: bool,
}

impl ColorPolicy {
    /// The policy for one output stream.
    pub(crate) fn for_stream(is_terminal: bool, no_color: Option<&str>) -> Self {
        Self {
            enabled: is_terminal && no_color.is_none(),
        }
    }

    /// Paints `text` for its role, or returns the same readable text
    /// when the stream has no color.
    pub(crate) fn paint(self, role: ColorRole, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{}m{text}\x1b[0m", role.sgr())
        } else {
            text.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_palette_uses_only_ansi_16_colors() {
        let policy = ColorPolicy::for_stream(true, None);
        assert_eq!(
            policy.paint(ColorRole::Error, "error"),
            "\x1b[1;31merror\x1b[0m"
        );
        assert_eq!(
            policy.paint(ColorRole::Warning, "warning"),
            "\x1b[1;33mwarning\x1b[0m"
        );
        assert_eq!(
            policy.paint(ColorRole::Info, "info"),
            "\x1b[1;36minfo\x1b[0m"
        );
    }

    #[test]
    fn color_is_disabled_for_non_tty_and_any_no_color_value() {
        assert_eq!(
            ColorPolicy::for_stream(false, None).paint(ColorRole::Error, "error"),
            "error"
        );
        for value in ["1", ""] {
            assert_eq!(
                ColorPolicy::for_stream(true, Some(value)).paint(ColorRole::Warning, "warning"),
                "warning"
            );
        }
    }
}
