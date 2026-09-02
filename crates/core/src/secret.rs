//! A value that must never reach a log, an event, a diagnostic or a
//! `Debug` dump: an environment variable's value, a bearer token.

/// Wraps a secret so it can be handed around without being printed.
/// `Debug` writes `[redacted]`; the value is reached only through
/// [`Secret::expose`], at the one place it is handed to the child
/// process or the wire. There is no `Display` and no `Serialize`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Secret(value)
    }

    /// The value itself, for the boundary that must see it.
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Secret(value)
    }
}

impl<T> std::fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debug_never_prints_the_value() {
        let secret = Secret::from("hunter2".to_string());
        assert_eq!(format!("{secret:?}"), "[redacted]");
        assert_eq!(secret.expose(), "hunter2");
    }
}
