//! Content hashes and commit ids as the log records them: lowercase hex,
//! parsed once at the frontier. A content hash is what this crate
//! computes over bytes; a commit id is what git or a forge reports. The
//! two types keep them from being swapped, and a value that is not hex
//! is refused with its rule instead of flowing on as text.

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::ids::{string_id, InvalidId};

const fn is_lower_hex(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while let Some((byte, tail)) = rest.split_first() {
        if !(byte.is_ascii_digit() || (b'a' <= *byte && *byte <= b'f')) {
            return false;
        }
        rest = tail;
    }
    true
}

/// A SHA-256 is exactly 32 bytes: 64 hex digits, nothing shorter passes
/// for one.
const CONTENT_HASH_RULE: &str = "64 lowercase hex digits, the SHA-256 of the content";

const fn is_content_hash(bytes: &[u8]) -> bool {
    bytes.len() == 64 && is_lower_hex(bytes)
}

/// Git prints an object id abbreviated to no fewer than 7 digits, in
/// full as 40 (SHA-1) or 64 (SHA-256).
const COMMIT_SHA_RULE: &str = "7 to 64 lowercase hex digits: a git object id, abbreviated or full";

const fn is_commit_sha(bytes: &[u8]) -> bool {
    bytes.len() >= 7 && bytes.len() <= 64 && is_lower_hex(bytes)
}

string_id!(
    /// The SHA-256 of some content, as `sha256_hex` renders it: the hash
    /// of a manifest, a workflow, an artifact's bytes, a context segment.
    ContentHash, what = "content hash", rule = CONTENT_HASH_RULE, check = is_content_hash
);

string_id!(
    /// A git object id as git or a forge reports it: the commit a run
    /// starts from, the head a review covers, the merge commit an
    /// approval landed as.
    CommitSha, what = "commit sha", rule = COMMIT_SHA_RULE, check = is_commit_sha
);

fn lower_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a `String` cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

impl ContentHash {
    /// The SHA-256 of `bytes`, hex-encoded: the one way this workspace
    /// produces a content hash, so every hash it records has this shape.
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(Cow::Owned(lower_hex(&Sha256::digest(bytes))))
    }
}

impl CommitSha {
    /// A commit id from its raw bytes, hex-encoded — `N` bytes render as
    /// `2 * N` digits, so any length git can print (4 to 32 bytes) is a
    /// valid id by construction. Real ids come from git's output through
    /// `FromStr`; this is for producers that mint ids, such as the mock
    /// forge.
    pub fn from_bytes<const N: usize>(bytes: &[u8; N]) -> Self {
        const {
            assert!(N >= 4 && N <= 32, "a git object id is 4 to 32 bytes");
        }
        Self(Cow::Owned(lower_hex(bytes)))
    }
}

/// Lowercase-hex SHA-256 of raw bytes — what `artifact_written` records
/// for a file's content (artifacts are verified by existence and hash,
/// never by format).
pub fn sha256_hex(bytes: &[u8]) -> ContentHash {
    ContentHash::sha256(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_content_hash_is_exactly_a_sha256() {
        let hash = sha256_hex(b"content");
        assert_eq!(hash.as_str().len(), 64);
        assert_eq!(hash.as_str().parse::<ContentHash>().unwrap(), hash);
        assert!(
            "deadbeef".parse::<ContentHash>().is_err(),
            "too short for a SHA-256"
        );
        assert!(
            format!("sha256:{hash}").parse::<ContentHash>().is_err(),
            "no prefix"
        );
        assert!(
            hash.as_str().to_uppercase().parse::<ContentHash>().is_err(),
            "lowercase only"
        );
    }

    #[test]
    fn a_commit_sha_is_what_git_prints() {
        assert!("deadbeef".parse::<CommitSha>().is_ok(), "an abbreviation");
        assert!("a".repeat(40).parse::<CommitSha>().is_ok(), "a full SHA-1");
        assert!(
            "f".repeat(64).parse::<CommitSha>().is_ok(),
            "a full SHA-256"
        );
        assert!(
            "abcdef".parse::<CommitSha>().is_err(),
            "shorter than git abbreviates"
        );
        assert!(
            "a".repeat(65).parse::<CommitSha>().is_err(),
            "longer than any object id"
        );
        assert!(
            "sha0000000000000000000000000000000000001"
                .parse::<CommitSha>()
                .is_err(),
            "not hex"
        );
        assert_eq!(
            CommitSha::from_bytes(&7u64.to_be_bytes()).as_str(),
            "0000000000000007"
        );
    }
}
