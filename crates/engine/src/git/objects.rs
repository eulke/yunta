//! The text a commit holds at many paths, read in one batch: one `git
//! cat-file` answers every path asked, in the order they were asked.

use std::collections::BTreeMap;
use std::path::Path;

use super::{exit_error, lossy, read, run_with, split_once, unreadable, GitError};
use crate::process::Supervision;

/// The text of each of `paths` at `commit`, by path: one `git cat-file
/// --batch` however many paths, read lossily. A path at which `commit`
/// holds no file is left out. A path holding a line break, which a batch
/// cannot name, is read alone with `show`, to which a path the commit
/// lacks is an error.
pub async fn blobs(
    dir: &Path,
    commit: &str,
    paths: &[String],
    sup: Supervision<'_>,
) -> Result<BTreeMap<String, String>, GitError> {
    // A batch reads each path to the `\n` that ends its line, and drops a
    // `\r` that stands before it.
    let (alone, batched): (Vec<&String>, Vec<&String>) =
        paths.iter().partition(|path| path.contains(['\n', '\r']));
    let mut texts = batch(dir, commit, &batched, sup).await?;
    for path in alone {
        let spec = format!("{commit}:{path}");
        texts.insert(path.clone(), read(dir, &["show", &spec], sup).await?);
    }
    Ok(texts)
}

/// The text of each of `paths` at `commit` that one `cat-file --batch`
/// reads, asked a path a line.
async fn batch(
    dir: &Path,
    commit: &str,
    paths: &[&String],
    sup: Supervision<'_>,
) -> Result<BTreeMap<String, String>, GitError> {
    let mut texts = BTreeMap::new();
    if paths.is_empty() {
        return Ok(texts);
    }
    let specs: Vec<String> = paths
        .iter()
        .map(|path| format!("{commit}:{path}"))
        .collect();
    let asked: String = specs.iter().map(|spec| format!("{spec}\n")).collect();
    let args = ["cat-file", "--batch"];
    let output = run_with(dir, &args, Some(asked.into_bytes()), sup).await?;
    if !output.status.success() {
        return Err(exit_error(dir, &args, &output));
    }
    let mut printed = output.stdout.as_slice();
    for (path, spec) in paths.iter().zip(&specs) {
        let Some(answer) = answer(&mut printed, spec) else {
            let said = split_once(printed, b'\n').map_or(printed, |(line, _)| line);
            let which = format!("is no answer to `{spec}`");
            return Err(unreadable(dir, &args, &lossy(said), &which));
        };
        if let Answer::File(text) = answer {
            texts.insert((*path).clone(), lossy(text));
        }
    }
    Ok(texts)
}

/// What a batch answers for one path.
#[derive(Debug, PartialEq, Eq)]
enum Answer<'a> {
    /// The file the commit holds there: its bytes.
    File(&'a [u8]),
    /// Nothing the reading keeps: no object stands there, or one that is no
    /// file, such as a directory.
    Nothing,
}

/// The answer at the front of `printed` for `spec`, taken off it: `<oid>
/// <type> <size>` and then that many bytes and a line break, or `<spec>
/// missing`. `None`, with `printed` left as it was, for any other.
fn answer<'a>(printed: &mut &'a [u8], spec: &str) -> Option<Answer<'a>> {
    let (header, rest) = split_once(printed, b'\n')?;
    if header == format!("{spec} missing").as_bytes() {
        *printed = rest;
        return Some(Answer::Nothing);
    }
    let mut fields = header.split(|&byte| byte == b' ');
    let (Some(_), Some(kind), Some(size), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return None;
    };
    let size: usize = std::str::from_utf8(size).ok()?.parse().ok()?;
    let content = rest.get(..size)?;
    *printed = rest.get(size..)?.strip_prefix(b"\n")?;
    Some(match kind {
        b"blob" => Answer::File(content),
        _ => Answer::Nothing,
    })
}

#[cfg(test)]
mod tests {
    use super::{answer, Answer};

    /// A batch's answers are read in turn, each taken off what git printed:
    /// a file's bytes, nothing for a directory or for a path the commit
    /// lacks, and no reading at all for an answer of any other shape.
    #[test]
    fn a_batch_answers_each_path_with_an_object_or_with_missing() {
        let mut printed: &[u8] =
            b"a1 blob 3\nab\n\nb2 tree 2\nxy\nc:gone.rs missing\nc:x.rs ambiguous\n";

        assert_eq!(answer(&mut printed, "c:a.rs"), Some(Answer::File(b"ab\n")));
        assert_eq!(answer(&mut printed, "c:dir"), Some(Answer::Nothing));
        assert_eq!(answer(&mut printed, "c:gone.rs"), Some(Answer::Nothing));
        assert_eq!(answer(&mut printed, "c:x.rs"), None);
        assert_eq!(printed, b"c:x.rs ambiguous\n");
    }

    /// An object that ends before the size its header names is no answer.
    #[test]
    fn an_object_shorter_than_its_size_is_no_answer() {
        let mut printed: &[u8] = b"a1 blob 10\nshort\n";

        assert_eq!(answer(&mut printed, "c:a.rs"), None);
        assert_eq!(printed, b"a1 blob 10\nshort\n");
    }
}
