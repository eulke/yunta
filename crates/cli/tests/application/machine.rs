//! The machine in the application, from the palette: doctor's findings, and upkeep, from
//! doctor, which says what collecting would reclaim and collects once it is confirmed.

use yunta_testkit::workflows::FINISHES;
use yunta_testkit::{stderr, yunta_at, Checkout, Terminal};

use super::{given_back, opened, taken_from_the_palette};

/// The config of a repository whose finished runs are kept no day, so upkeep has one to
/// reclaim.
const KEPT_NO_DAY: &str = "defaults:\n  isolation: none\nstorage:\n  retention_days: 0\n";

/// What doctor's findings stand under, whatever this machine holds: its headline.
const DOCTORS_HEAD: &str = "yunta doctor ·";

/// Bare `yunta` on a repository with one run that finished, kept no day, at its field.
fn on_the_field() -> (Checkout, Terminal) {
    let checkout = Checkout::new()
        .config(KEPT_NO_DAY)
        .workflow("done", FINISHES)
        .committed();
    let ran = yunta_at!(&checkout, &["run", "done.yaml"]);
    assert!(ran.status.success(), "{}", stderr(&ran));
    let terminal = opened(&checkout);
    (checkout, terminal)
}

#[test]
fn doctor_opens_from_the_palette_on_its_findings() {
    let (_checkout, mut terminal) = on_the_field();
    taken_from_the_palette(&mut terminal, "yunta doctor");
    terminal.wait_on_screen(DOCTORS_HEAD, "doctor's findings never stood");
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}

/// From doctor, `u` opens upkeep on what collecting would reclaim; `c` asks first, and,
/// confirmed, collects the run as `yunta gc` does.
#[test]
fn upkeep_says_what_it_would_reclaim_and_collects_once_confirmed() {
    let (checkout, mut terminal) = on_the_field();
    let [id] = yunta_testkit::run_ids(&checkout.home)
        .try_into()
        .expect("one run");
    taken_from_the_palette(&mut terminal, "yunta doctor");
    terminal.wait_on_screen(DOCTORS_HEAD, "doctor's findings never stood");
    terminal.keys("u");
    terminal.wait_on_screen(
        "would reclaim 1 run",
        "upkeep never said what it would reclaim",
    );
    terminal.keys("c");
    terminal.wait_on_screen("removed for good", "collecting was never asked first");
    terminal.keys("\r");
    yunta_testkit::wait_until(
        || !yunta_testkit::run_ids(&checkout.home).contains(&id),
        || "the run was never collected".to_string(),
    );
    terminal.keys("q");
    assert_eq!(given_back(&mut terminal), Some(0));
}
