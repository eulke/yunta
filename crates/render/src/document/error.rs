//! See [`super`]. A refusal as a program reads it under `--json`, and a
//! command a reader can type next.

use serde::Serialize;

use super::SCHEMA_VERSION;

/// A refusal as a program reads it under `--json`: one document, and the
/// version of the schema it was written under.
#[derive(Serialize)]
pub struct ErrorDocument {
    schema_version: u32,
    error: ErrorBody,
}

/// What a refusal says, where it looked, and the commands that go on from
/// here.
#[derive(Serialize)]
struct ErrorBody {
    said: String,
    detail: Option<String>,
    next: Vec<NextCommand>,
}

/// A command a person or a program can type next, and what it is for.
#[derive(Serialize)]
pub struct NextCommand {
    command: String,
    why: String,
}

impl NextCommand {
    /// A command a reader can type next, with the one line that says what it does.
    pub fn new(command: String, why: &str) -> Self {
        Self {
            command,
            why: why.to_string(),
        }
    }
}

impl ErrorDocument {
    /// A refusal or a failure as a program is told it: what it says, where it
    /// looked, and each command that goes on with what it does.
    pub fn new(said: String, detail: Option<String>, next: Vec<(String, &'static str)>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            error: ErrorBody {
                said,
                detail,
                next: next
                    .into_iter()
                    .map(|(command, why)| NextCommand::new(command, why))
                    .collect(),
            },
        }
    }
}
