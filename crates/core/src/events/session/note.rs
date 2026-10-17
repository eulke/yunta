//! A note a session wrote, as the log keeps it: its size and the head of its
//! digest, never its words.

use serde::{Deserialize, Serialize};

/// The hex digits of a note's digest the log keeps: enough to audit a note a
/// session claims to have written, never enough to read it back.
const NOTE_DIGEST_HEX: usize = 12;

/// A note a session wrote, as the log keeps it: its size and the head of its digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoteDigest {
    pub bytes: u64,
    pub sha256: String,
}

impl NoteDigest {
    /// The size and the digest head of a note's text.
    pub fn of(text: &str) -> Self {
        let hash = crate::sha256_hex(text.as_bytes()).to_string();
        NoteDigest {
            bytes: text.len() as u64,
            sha256: hash[..NOTE_DIGEST_HEX].to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_keeps_its_size_and_the_head_of_its_digest() {
        let note = NoteDigest::of("thinking about SECRET-TOKEN-123 carefully");
        assert_eq!(
            note,
            NoteDigest {
                bytes: 41,
                sha256: "052f9656f4d7".into(),
            }
        );
    }

    #[test]
    fn a_note_digest_never_holds_the_words_of_its_note() {
        let note = NoteDigest::of("thinking about SECRET-TOKEN-123 carefully");
        let written = serde_json::to_string(&note).unwrap();
        assert!(!written.contains("SECRET"), "{written}");
    }
}
