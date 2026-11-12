//! A command's error as the page a person reads: a refusal's is render's,
//! and a failure says its sentence with what caused it. An agent reads no
//! page: the same refusal reaches it as plain text, through
//! [`CliError::said_to`].

use surco::components::{Next, Refusal};
use surco::doc::Doc;
use surco::look::Look;
use yunta_render::command::Reader;
use yunta_render::page::Page;

use super::CliError;

/// A command's error as a person reads it on stderr: a refusal's page, with
/// each problem it found and the commands that go on, or a failure's
/// sentence.
impl Page for CliError {
    fn document(&self, look: &Look) -> Doc {
        match self {
            CliError::Refused(refused) => refused.document(look),
            failed => Doc::new()
                .with(Refusal {
                    said: failed.said_to(Reader::Person),
                    detail: None,
                })
                .with(Next { steps: Vec::new() }),
        }
    }
}
