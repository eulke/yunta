//! A frame in ASCII holds ASCII alone, a character a cell: a terminal that names no UTF-8
//! locale draws any other character as several cells, and the row it stands in passes its
//! width. The words a session or a pack recorded are drawn in their ASCII form there too.

use super::targets;

/// Every landed frame of the ASCII environment holds ASCII alone. Each row outside ASCII is
/// named with its frame.
#[test]
fn every_landed_frame_in_ascii_holds_ascii_alone() {
    let mut frames = 0;
    let mut outside = Vec::new();
    for entry in std::fs::read_dir(targets()).expect("the landed frames") {
        let path = entry.expect("a landed frame").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !name.contains(".ascii-") {
            continue;
        }
        frames += 1;
        let frame = std::fs::read_to_string(&path).expect("a landed frame");
        outside.extend(
            (frame.lines().enumerate())
                .filter(|(_, row)| !row.is_ascii())
                .map(|(at, row)| format!("{name} row {}: {row}", at + 1)),
        );
    }
    assert!(frames > 0, "no frame of the ASCII environment is landed");
    assert!(
        outside.is_empty(),
        "rows outside ASCII:\n{}",
        outside.join("\n")
    );
}
