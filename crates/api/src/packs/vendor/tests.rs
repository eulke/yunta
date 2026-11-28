use std::path::PathBuf;

use super::PackError;

/// A symbolic link found in a pack's own tree, read at its audit or against its lock, is said
/// as what was found there, a link a pack never ships, and not as what vendoring does.
#[test]
fn a_link_in_a_pack_s_tree_is_said_as_found() {
    let link = PathBuf::from(".yunta/packs/yunta/fragua/workflows/link.yaml");
    let said = PackError::Symlink { path: link }.to_string();
    assert_eq!(
        said,
        "`.yunta/packs/yunta/fragua/workflows/link.yaml` is a symbolic link in the pack's tree, \
         which a pack never ships"
    );
}
