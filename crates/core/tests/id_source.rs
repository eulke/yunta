//! Run ids come from an injected source, the way time comes from an
//! injected clock: production mints ULIDs stamped with the instant it is
//! given; tests inject a sequential source and get reproducible ids.

use chrono::{DateTime, Utc};
use yunta_core::{IdSource, SeqIdSource, SystemIdSource};

fn instant(rfc3339: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
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
