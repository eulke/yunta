//! The CLI's key table, key by key: each key gives the application the stroke the CLI's
//! reader gives it; a release gives nothing; Ctrl-Z suspends and is no stroke; a paste is
//! typed while a line is, and nothing while none is.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use yunta_ui::prompt::Stroke;

use super::{pasted, stroke, suspends};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn with_control(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

#[test]
fn each_key_gives_the_stroke_the_clis_reader_gives_it() {
    let table = [
        (press(KeyCode::Char('a')), Stroke::Insert('a')),
        (
            KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT),
            Stroke::Insert('A'),
        ),
        (press(KeyCode::Char('é')), Stroke::Insert('é')),
        (press(KeyCode::Char(' ')), Stroke::Insert(' ')),
        (press(KeyCode::Enter), Stroke::Enter),
        (press(KeyCode::Esc), Stroke::Decline),
        (with_control('c'), Stroke::Interrupt),
        (press(KeyCode::Backspace), Stroke::Backspace),
        (press(KeyCode::Delete), Stroke::Delete),
        (press(KeyCode::Left), Stroke::Left),
        (press(KeyCode::Right), Stroke::Right),
        (press(KeyCode::Up), Stroke::Up),
        (press(KeyCode::Down), Stroke::Down),
        (press(KeyCode::Home), Stroke::Home),
        (press(KeyCode::End), Stroke::End),
        (press(KeyCode::PageUp), Stroke::PageUp),
        (press(KeyCode::PageDown), Stroke::PageDown),
        (press(KeyCode::Tab), Stroke::Tab),
        (
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            Stroke::BackTab,
        ),
        (with_control('x'), Stroke::Ignored),
        (press(KeyCode::F(5)), Stroke::Ignored),
    ];
    for (key, expected) in table {
        assert_eq!(stroke(key), Some(expected), "{key:?}");
        assert!(!suspends(key), "{key:?}");
    }
}

#[test]
fn a_release_gives_nothing() {
    let mut released = press(KeyCode::Char('q'));
    released.kind = KeyEventKind::Release;
    assert_eq!(stroke(released), None);
    let mut repeated = press(KeyCode::Down);
    repeated.kind = KeyEventKind::Repeat;
    assert_eq!(stroke(repeated), Some(Stroke::Down), "a key held repeats");
}

#[test]
fn ctrl_z_suspends_and_is_no_stroke() {
    assert!(suspends(with_control('z')));
    assert_eq!(stroke(with_control('z')), None);
}

#[test]
fn a_paste_is_typed_while_a_line_is_and_nothing_otherwise() {
    assert_eq!(
        pasted("ab\r\nc\td", true),
        [
            Stroke::Insert('a'),
            Stroke::Insert('b'),
            Stroke::PastedBreak,
            Stroke::Insert('c'),
            Stroke::Insert(' '),
            Stroke::Insert('d'),
        ]
    );
    assert_eq!(pasted("a\nb", true)[1], Stroke::PastedBreak);
    assert!(pasted("ab\ncd", false).is_empty());
}
