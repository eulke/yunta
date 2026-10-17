//! Words an older log stored where a field now holds data.

use serde::{Deserialize, Serialize};

/// Words an older log stored where the field now holds data: read, shown as
/// written, and written back as read, so an export of an older run keeps
/// every word it had. Production code only gets one by reading a log, so
/// no code of the workspace writes words into a log through it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct LoggedWords(String);

impl LoggedWords {
    /// The words as the log stored them.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The words a wire type of this crate read off a log.
    pub(crate) fn read(text: String) -> Self {
        LoggedWords(text)
    }
}

impl std::fmt::Display for LoggedWords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for LoggedWords {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for LoggedWords {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Words a test states as an older log would have stored them.
#[cfg(any(test, feature = "testkit"))]
impl From<&str> for LoggedWords {
    fn from(text: &str) -> Self {
        LoggedWords(text.to_string())
    }
}
