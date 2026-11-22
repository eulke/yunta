//! Run ids come from an injected source, the way time comes from an
//! injected clock: production mints ULIDs stamped with the instant it is
//! given; tests inject a sequential source and get reproducible ids.

use jiff::Timestamp;
use yunta_core::{IdSource, SystemIdSource};
use yunta_testkit_core::SeqIdSource;

fn instant(rfc3339: &str) -> Timestamp {
    rfc3339.parse::<Timestamp>().unwrap()
}

const CROCKFORD: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[test]
fn system_id_source_mints_ulids_ordered_by_the_instant() {
    let source = SystemIdSource;
    let earlier = source.mint_run_id(instant("2026-09-02T10:00:00Z"));
    let later = source.mint_run_id(instant("2026-09-02T10:00:01Z"));
    for id in [&earlier, &later] {
        assert_eq!(id.as_str().len(), 26, "{id}");
        assert!(
            id.as_str().chars().all(|c| CROCKFORD.contains(c)),
            "{id} is not Crockford base32"
        );
    }
    assert!(
        earlier.as_str() < later.as_str(),
        "{earlier} must sort before {later}"
    );
}

/// The first ten characters are the instant's millisecond since the epoch,
/// in Crockford base32; an instant at or before the epoch stamps zero.
#[test]
fn system_id_source_stamps_the_millisecond_and_zero_before_the_epoch() {
    let source = SystemIdSource;
    let stamp = |at: &str| source.mint_run_id(instant(at)).as_str()[..10].to_string();
    assert_eq!(stamp("2026-09-02T10:00:00.123Z"), "01M1GRZABV");
    assert_eq!(stamp("1970-01-01T00:00:00.001Z"), "0000000001");
    assert_eq!(stamp("1970-01-01T00:00:00Z"), "0000000000");
    assert_eq!(stamp("1969-12-31T23:59:59.5Z"), "0000000000");
    assert_eq!(stamp("1900-01-01T00:00:00Z"), "0000000000");
}

#[test]
fn system_id_source_never_mints_the_same_id_twice_for_one_instant() {
    let source = SystemIdSource;
    let at = instant("2026-09-02T10:00:00Z");
    assert_ne!(source.mint_run_id(at), source.mint_run_id(at));
}

#[test]
fn seq_id_source_counts_from_one_under_its_prefix_and_ignores_the_instant() {
    let source = SeqIdSource::new("minted");
    assert_eq!(
        source.mint_run_id(instant("2026-09-02T10:00:00Z")),
        "minted-1"
    );
    assert_eq!(
        source.mint_run_id(instant("2020-01-01T00:00:00Z")),
        "minted-2"
    );
}
