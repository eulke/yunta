use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::said::{Said, Voices, TEXT_MOST};
use yunta_core::{NodeId, RunId, Seq, SessionId};

const RUN: &str = "01M6Q6X3VA7E9X676XY7CDQRA0";

fn said(seq: u64, text: &str) -> Said {
    Said {
        run_id: RunId::from(RUN),
        node_id: NodeId::from("implement"),
        session_id: SessionId::from_static("session-1"),
        task_id: None,
        seq: Seq::from(seq),
        at: chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap_or_default(),
        text: text.to_string(),
        cut: false,
    }
}

/// A directory with a short path, so a socket under it binds on any host.
fn short_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("w")
        .tempdir_in("/tmp")
        .unwrap()
}

/// `count` texts a reader of `path`'s socket is handed.
async fn heard(hearing: &mut crate::said::Hearing, count: usize) -> Vec<String> {
    let mut texts = Vec::new();
    while texts.len() < count {
        let Some(heard) = hearing.next().await else {
            break;
        };
        texts.push(heard.said.text);
    }
    texts
}

/// A path of exactly `bytes` bytes under `root`.
fn path_of(root: &Path, bytes: usize) -> PathBuf {
    let base = root.join("x");
    let pad = bytes - base.as_os_str().len();
    root.join("x".repeat(pad + 1))
}

#[tokio::test]
async fn a_reader_of_the_socket_gets_what_the_voices_hold_then_what_comes() {
    let dir = short_dir();
    let voices = Arc::new(Voices::new());
    voices.hand(said(1, "one"));
    voices.hand(said(2, "two"));
    let path = dir.path().join("live.sock");
    let _socket = Socket::bind(&path, Arc::clone(&voices), user::uid()).unwrap();

    let mut hearing = crate::said::connect(&path).await.unwrap();
    voices.hand(said(3, "three"));

    assert_eq!(heard(&mut hearing, 3).await, ["one", "two", "three"]);
}

#[test]
fn the_scratch_path_is_kept_up_to_the_limit_and_the_short_one_past_it() {
    let root = short_dir();
    let short = short_dir();
    let run = RunId::from(RUN);
    let at_limit = path_of(root.path(), PATH_MOST);
    let past_limit = path_of(root.path(), PATH_MOST + 1);

    assert_eq!(
        place(&at_limit, &run, short.path(), user::uid()),
        Some(at_limit.clone())
    );
    let placed = place(&past_limit, &run, short.path(), user::uid());
    let private = short.path().join(format!("yunta-{}", user::uid()));
    assert_eq!(placed, Some(private.join(format!("{RUN}.sock"))));
    let mode = std::fs::metadata(&private).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn a_short_directory_that_is_not_private_to_the_user_gives_no_path() {
    let root = short_dir();
    let run = RunId::from(RUN);
    let long = path_of(root.path(), PATH_MOST + 1);
    let uid = user::uid();

    let open = short_dir();
    let dir = open.path().join(format!("yunta-{uid}"));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(place(&long, &run, open.path(), uid), None, "open to others");

    let linked = short_dir();
    let elsewhere = short_dir();
    std::fs::set_permissions(elsewhere.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), linked.path().join(format!("yunta-{uid}")))
        .unwrap();
    assert_eq!(place(&long, &run, linked.path(), uid), None, "a link");

    // A directory this process made belongs to its uid, so to the code it is
    // another user's when another uid is the one asking.
    let theirs = short_dir();
    assert!(place(&long, &run, theirs.path(), uid).is_some());
    assert_eq!(
        place(&long, &run, theirs.path(), uid + 1),
        None,
        "another user's"
    );
}

#[tokio::test]
async fn binding_replaces_a_stale_socket_and_refuses_anything_else() {
    let dir = short_dir();
    let voices = Arc::new(Voices::new());
    let stale = dir.path().join("stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    assert!(std::fs::symlink_metadata(&stale)
        .unwrap()
        .file_type()
        .is_socket());

    let socket = Socket::bind(&stale, Arc::clone(&voices), user::uid()).unwrap();
    let mode = std::fs::metadata(&stale).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    voices.hand(said(1, "fresh"));
    let mut hearing = crate::said::connect(&stale).await.unwrap();
    assert_eq!(heard(&mut hearing, 1).await, ["fresh"]);
    drop(socket);

    let file = dir.path().join("file.sock");
    std::fs::write(&file, "a person's file").unwrap();
    assert!(Socket::bind(&file, Arc::clone(&voices), user::uid()).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a person's file");
}

#[tokio::test]
async fn dropping_the_socket_removes_it_and_ends_every_reader() {
    let dir = short_dir();
    let voices = Arc::new(Voices::new());
    let path = dir.path().join("live.sock");
    let socket = Socket::bind(&path, Arc::clone(&voices), user::uid()).unwrap();
    voices.hand(said(1, "one"));
    let mut hearing = crate::said::connect(&path).await.unwrap();
    assert_eq!(heard(&mut hearing, 1).await, ["one"]);

    drop(socket);

    assert!(hearing.next().await.is_none());
    assert!(!path.exists());
}

#[tokio::test]
async fn a_reader_that_never_reads_holds_back_no_other_reader() {
    let dir = short_dir();
    let voices = Arc::new(Voices::new());
    let path = dir.path().join("live.sock");
    let _socket = Socket::bind(&path, Arc::clone(&voices), user::uid()).unwrap();
    let _stuck = crate::said::connect(&path).await.unwrap();
    let mut reading = crate::said::connect(&path).await.unwrap();

    // Far more than a socket's buffer holds: the stuck reader's writer waits,
    // and only it does.
    let long = "a".repeat(TEXT_MOST);
    for seq in 1..=300 {
        voices.hand(said(seq, &long));
        let heard = reading.next().await.unwrap();
        assert_eq!((heard.said.seq, heard.missed), (Seq::from(seq), 0));
    }
}

#[tokio::test]
async fn a_reader_skips_a_line_it_cannot_read_and_keeps_reading() {
    use std::io::Write;
    let dir = short_dir();
    let path = dir.path().join("newer.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let frame = crate::said::Heard {
        said: said(9, "still heard"),
        missed: 0,
    };
    let served = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        writeln!(stream, r#"{{"v":2,"from":"a newer engine"}}"#).unwrap();
        writeln!(stream, "not a frame").unwrap();
        writeln!(stream, "{}", frame.line()).unwrap();
    });

    let mut hearing = crate::said::connect(&path).await.unwrap();

    assert_eq!(heard(&mut hearing, 2).await, ["still heard"]);
    served.join().unwrap();
}

#[test]
fn only_the_engines_own_user_is_admitted() {
    assert!(admits(Some(501), 501));
    assert!(!admits(Some(502), 501));
    assert!(!admits(None, 501));
}
