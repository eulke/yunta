//! The redactor's verdict on an event: the payload the log writes, with
//! every secret taken out, or a refusal when that payload would not read
//! back as the same payload.

use super::EventPayload;
use crate::Redactor;

/// A payload whose redacted form does not read back as the same payload.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unredactable payload of kind `{kind}`")]
pub struct Unredactable {
    pub kind: String,
}

impl Redactor {
    /// `payload` with every secret taken out, as the same typed payload; the
    /// payload itself when nothing needed taking out. `Err` when it does not
    /// read back.
    pub fn payload(&self, payload: EventPayload) -> Result<EventPayload, Unredactable> {
        if self.is_empty() {
            return Ok(payload);
        }
        let kind = payload.kind_name();
        let value =
            serde_json::to_value(&payload).map_err(|_| Unredactable { kind: kind.into() })?;
        let redacted = self.json(value.clone());
        if redacted == value {
            return Ok(payload);
        }
        serde_json::from_value(redacted).map_err(|_| Unredactable { kind: kind.into() })
    }
}
