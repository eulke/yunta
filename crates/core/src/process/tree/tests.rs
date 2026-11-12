use super::*;

fn process(pid: i32, parent: i32, group: i32) -> Process {
    Process { pid, parent, group }
}

/// A tool command two levels under the CLI, in a group of its own, is named;
/// the CLI's own group is not.
#[test]
fn a_group_two_levels_down_is_named() {
    let table = [
        process(100, 10, 100),
        process(101, 100, 101),
        process(102, 101, 101),
    ];

    assert_eq!(under(&table, 100, 10), vec![101]);
}

/// A process that joined the caller's group, or a group of 1, is never named,
/// though the walk still goes through it to what is under it.
#[test]
fn the_callers_group_and_group_one_are_never_named() {
    let table = [
        process(100, 10, 100),
        process(101, 100, 10),
        process(102, 101, 102),
        process(103, 100, 1),
    ];

    assert_eq!(under(&table, 100, 10), vec![102]);
}

/// The walk starts from every member of the group, so a member whose leader
/// is gone still leads to what it made.
#[test]
fn a_member_whose_leader_left_still_leads_down() {
    let table = [process(200, 1, 100), process(201, 200, 201)];

    assert_eq!(under(&table, 100, 10), vec![201]);
}

/// A loop of parent links, which a table read while processes come and go
/// can show, ends the walk instead of hanging it.
#[test]
fn a_loop_of_parent_links_ends() {
    let table = [
        process(100, 10, 100),
        process(101, 102, 101),
        process(102, 101, 102),
    ];

    assert_eq!(under(&table, 100, 10), Vec::<i32>::new());
}
