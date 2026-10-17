//! Whether a session is offered the network: the node's own word decides when it
//! gives one, and the project's default decides when it gives none.

use super::*;
use yunta_core::network_asked;

#[test]
fn a_session_is_offered_the_network_when_neither_the_node_nor_the_project_says() {
    assert!(network_asked(None, None));
}

#[test]
fn a_node_word_wins_over_the_project_default_in_both_directions() {
    let closed = Some(NetworkPermissions { default: false });
    let open = Some(NetworkPermissions { default: true });
    assert!(network_asked(Some(true), closed));
    assert!(!network_asked(Some(false), open));
}

#[test]
fn the_project_default_applies_when_the_node_says_nothing() {
    assert!(!network_asked(
        None,
        Some(NetworkPermissions { default: false })
    ));
    assert!(network_asked(
        None,
        Some(NetworkPermissions { default: true })
    ));
}
