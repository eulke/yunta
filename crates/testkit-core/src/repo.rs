//! What a test asks git for directly. A test that checks an id the code
//! under test computes asks git for the same id as well, so a computation
//! that disagrees with git fails the test instead of agreeing with itself.

use std::io::Write;
use std::process::{Command, Stdio};

/// The blob id `git hash-object` gives `bytes`: the `sha` a forge reports
/// for a stored file with those bytes, as git computes it.
pub fn blob_id(bytes: &[u8]) -> String {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("git runs");
    // The handle is dropped after the write, which closes stdin and ends the input.
    child
        .stdin
        .take()
        .expect("git has a stdin pipe")
        .write_all(bytes)
        .expect("git reads the content");
    let output = child.wait_with_output().expect("git finishes");
    String::from_utf8(output.stdout)
        .expect("an id is text")
        .trim()
        .to_string()
}
