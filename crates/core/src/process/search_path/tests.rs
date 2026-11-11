use std::path::{Path, PathBuf};

use super::SearchPath;

/// A file at `path` holding a script, with the execute bits `mode` sets.
#[cfg(unix)]
fn script(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A bare name is found where the system's own search would start it: the
/// first directory holding a file by that name that a process may execute. A
/// directory by that name, and a file nobody may execute, are passed over.
#[cfg(unix)]
#[test]
fn a_program_is_the_first_executable_file_by_its_name() {
    let root = tempfile::tempdir().unwrap();
    let dirs: Vec<PathBuf> = ["directory", "unexecutable", "first", "second"]
        .iter()
        .map(|name| root.path().join(name))
        .collect();
    for dir in &dirs {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::create_dir_all(dirs[0].join("cc")).unwrap();
    script(&dirs[1].join("cc"), 0o644);
    script(&dirs[2].join("cc"), 0o755);
    script(&dirs[3].join("cc"), 0o755);
    let search = SearchPath::new(dirs.clone());

    assert_eq!(search.find("cc"), Some(dirs[2].join("cc")));
    assert_eq!(search.find("ld"), None);
}

/// A path names its program: it is found where a file stands there, whatever
/// the search holds, and nowhere else.
#[cfg(unix)]
#[test]
fn a_path_names_its_program_wherever_the_search_looks() {
    let root = tempfile::tempdir().unwrap();
    let program = root.path().join("claude");
    script(&program, 0o755);
    let nowhere = SearchPath::default();

    assert_eq!(nowhere.find(&program), Some(program.clone()));
    assert_eq!(nowhere.find(root.path().join("codex")), None);
    assert_eq!(nowhere.find("claude"), None);
}

/// The directories a `PATH` value names read back as that value.
#[cfg(unix)]
#[test]
fn a_path_variable_reads_back_as_its_value() {
    let search = SearchPath::new(vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
    let value = search.joined().unwrap();

    assert_eq!(value, "/usr/bin:/bin");
    assert_eq!(SearchPath::of_var(&value), search);
    assert_eq!(SearchPath::default().joined(), None);
}
