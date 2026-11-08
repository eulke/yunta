use super::*;

#[test]
fn the_word_a_run_is_called_by_decides_the_group_it_is_listed_under() {
    // A log that stopped making sense needs a person as much as a
    // decision does: nothing moves it on its own again.
    for word in [Standing::NeedsYou, Standing::Broken] {
        assert_eq!(Group::of(word, false), Group::NeedsYou, "{word}");
    }
    for word in [Standing::Created, Standing::Running, Standing::Waiting] {
        assert_eq!(Group::of(word, false), Group::Running, "{word}");
    }
    assert_eq!(Group::of(Standing::Stopped, false), Group::Stopped);
    for word in [
        Standing::Finished,
        Standing::Reported,
        Standing::Failed,
        Standing::Stopped,
        Standing::Promoted,
    ] {
        assert_eq!(Group::of(word, true), Group::Closed, "{word}");
    }
}

#[test]
fn every_word_a_run_can_be_called_by_has_a_group() {
    // The listing is an inbox: a run whose word fell through would
    // be a run nobody goes looking for.
    let groups: Vec<Group> = Standing::ALL
        .into_iter()
        .map(|word| Group::of(word, false))
        .collect();
    assert_eq!(groups.len(), Standing::ALL.len());
}
