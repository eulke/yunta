use super::*;

/// A frame of one run's one session, saying `text` at the log's `seq`.
fn said(seq: u64, text: &str) -> Said {
    Said {
        run_id: RunId::from("01M6Q6X3VA7E9X676XY7CDQRA0"),
        node_id: NodeId::from("implement"),
        session_id: SessionId::from_static("session-1"),
        task_id: Some(TaskId::from_static("T001")),
        seq: Seq::from(seq),
        at: chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap_or_default(),
        text: text.to_string(),
        cut: false,
    }
}

/// The texts `listening` is handed, up to `count` of them.
async fn texts(listening: &mut Listening, count: usize) -> Vec<String> {
    let mut texts = Vec::new();
    while texts.len() < count {
        let Some(heard) = listening.next().await else {
            break;
        };
        texts.push(heard.said.text);
    }
    texts
}

#[tokio::test]
async fn a_reader_gets_the_recent_past_then_what_comes_and_nothing_twice() {
    let voices = Voices::new();
    voices.hand(said(1, "one"));
    voices.hand(said(2, "two"));
    let mut first = voices.listen();
    voices.hand(said(3, "three"));
    let mut second = voices.listen();
    voices.hand(said(4, "four"));

    assert_eq!(texts(&mut first, 4).await, ["one", "two", "three", "four"]);
    assert_eq!(texts(&mut second, 4).await, ["one", "two", "three", "four"]);
}

#[tokio::test]
async fn the_recent_past_keeps_the_last_frames_up_to_its_count() {
    let voices = Voices::new();
    for seq in 1..=(KEPT_FRAMES as u64 + 6) {
        voices.hand(said(seq, &format!("said {seq}")));
    }
    let mut listening = voices.listen();
    voices.hand(said(1_000, "after"));

    let heard = texts(&mut listening, KEPT_FRAMES + 1).await;
    assert_eq!(heard.len(), KEPT_FRAMES + 1);
    assert_eq!(heard.first().map(String::as_str), Some("said 7"));
    assert_eq!(heard.last().map(String::as_str), Some("after"));
}

#[tokio::test]
async fn the_recent_past_keeps_the_last_frames_up_to_its_bytes() {
    let voices = Voices::new();
    let longest = "a".repeat(TEXT_MOST);
    let fit = KEPT_BYTES / TEXT_MOST;
    for seq in 1..=(fit as u64 + 3) {
        voices.hand(said(seq, &longest));
    }
    voices.hand(said(100, "last"));
    let mut listening = voices.listen();
    voices.hand(said(101, "after"));

    // The short last frame took the room of the oldest long one, so the
    // past is one long frame fewer than the bytes hold, then the last two.
    let heard = texts(&mut listening, fit + 1).await;
    assert_eq!(
        heard.iter().filter(|text| text.len() == TEXT_MOST).count(),
        fit - 1
    );
    assert_eq!(&heard[fit - 1..], ["last", "after"]);
}

#[tokio::test]
async fn a_long_text_is_cut_at_a_character_boundary_and_says_so() {
    let voices = Voices::new();
    // The two bytes of `é` straddle the bound, so the cut leaves it out whole.
    let long = format!("{}é and more", "a".repeat(TEXT_MOST - 1));
    voices.hand(said(1, &long));
    voices.hand(said(2, "short"));
    let mut listening = voices.listen();

    let cut = listening.next().await.map(|heard| heard.said);
    let kept = listening.next().await.map(|heard| heard.said);
    assert_eq!(
        cut.as_ref().map(|said| said.text.len()),
        Some(TEXT_MOST - 1)
    );
    assert_eq!(cut.map(|said| said.cut), Some(true));
    assert_eq!(kept.as_ref().map(|said| said.text.as_str()), Some("short"));
    assert_eq!(kept.map(|said| said.cut), Some(false));
}

#[tokio::test]
async fn a_reader_that_falls_behind_is_told_how_many_it_missed() {
    let voices = Voices::new();
    let mut listening = voices.listen();
    // Handing over returns at once though this reader takes nothing.
    for seq in 1..=(READER_QUEUE as u64 + 5) {
        voices.hand(said(seq, &format!("said {seq}")));
    }

    let first = listening.next().await;
    assert_eq!(first.as_ref().map(|heard| heard.missed), Some(5));
    assert_eq!(
        first.map(|heard| heard.said.text),
        Some("said 6".to_string())
    );
    let next = listening.next().await;
    assert_eq!(next.map(|heard| heard.missed), Some(0));
}

#[tokio::test]
async fn a_reader_hears_the_end_when_the_voices_are_gone() {
    let voices = Voices::new();
    voices.hand(said(1, "one"));
    let mut listening = voices.listen();
    drop(voices);

    assert_eq!(texts(&mut listening, 2).await, ["one"]);
}

#[test]
fn a_frame_survives_its_line() {
    let heard = Heard {
        said: said(7, "line one\nline \"two\""),
        missed: 3,
    };
    let line = heard.line();

    assert!(
        line.starts_with(&format!("{{\"v\":{LINE_VERSION},")),
        "{line}"
    );
    assert!(!line.contains('\n'), "{line}");
    assert_eq!(Heard::read(&line).ok().flatten(), Some(heard));
}

#[test]
fn a_line_of_a_version_the_reader_does_not_know_is_skipped() {
    let newer = r#"{"v":2,"whatever":"comes next"}"#;

    assert_eq!(Heard::read(newer).ok(), Some(None));
    assert!(Heard::read("not json").is_err());
}
