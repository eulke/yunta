//! What a call's words come to, against a checkout that holds one file. Every
//! expected word here is written by hand from what a reader may be told.

use yunta_core::events::ToolAction::{self, Edit, Fetch, List, Read, Run, Search, Write};

use super::{of_command, of_path, Checkout};

/// A checkout holding `src/lib.rs`.
fn checkout() -> (tempfile::TempDir, Checkout) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    let checkout = Checkout::at(dir.path());
    (dir, checkout)
}

/// Each command of `table` reads as its action and shows what it says.
fn reads(table: &[(&str, ToolAction, Option<&str>)]) {
    let (_dir, checkout) = checkout();
    for (command, action, shows) in table {
        let (read, target) = of_command(command, &checkout);
        assert_eq!(read, *action, "{command}");
        assert_eq!(target.display(), *shows, "{command}");
    }
}

#[test]
fn a_program_of_the_table_shows_its_name_and_a_subcommand_it_knows() {
    reads(&[
        ("cargo test --quiet -p yunta", Run, Some("cargo test")),
        ("git diff --stat", Run, Some("git diff")),
        ("npm run build", Run, Some("npm run")),
        ("cargo +nightly test", Run, Some("cargo")),
        ("git -C /tmp/secret diff", Run, Some("git")),
        ("cargo frobnicate", Run, Some("cargo")),
        ("make release", Run, Some("make")),
        ("pytest tests/secret_test.py", Run, Some("pytest")),
        ("/usr/bin/git status", Run, Some("git status")),
    ]);
}

#[test]
fn a_program_out_of_the_table_shows_nothing() {
    reads(&[
        ("psql postgres://admin:hunter2@db/prod", Run, None),
        ("./deploy.sh --token hunter2", Run, None),
        ("bash script.sh", Run, None),
        ("", Run, None),
    ]);
}

#[test]
fn a_file_shows_only_where_the_checkout_holds_it() {
    reads(&[
        ("sed -n 1,40p src/lib.rs", Read, Some("src/lib.rs")),
        ("cat ./src/lib.rs", Read, Some("src/lib.rs")),
        ("cat src/lib.rs | head -5", Read, Some("src/lib.rs")),
        ("head -n 5 no/such/file", Read, None),
        ("cat ../outside", Read, None),
        ("cat /etc/hosts", Read, None),
        ("sed -i s/a/b/ src/lib.rs", Edit, Some("src/lib.rs")),
        ("apply_patch", Edit, None),
    ]);
}

#[test]
fn what_a_session_looks_for_or_fetches_is_never_shown() {
    reads(&[
        ("rg hunter2 src/lib.rs", Search, None),
        ("find . -name secret", Search, None),
        ("ls -la src", List, None),
        (
            "curl -H 'Authorization: hunter2' https://example.com",
            Fetch,
            None,
        ),
    ]);
}

#[test]
fn a_wrapper_is_opened_and_what_only_prepares_is_skipped() {
    reads(&[
        ("bash -lc 'cargo test'", Run, Some("cargo test")),
        (
            "/bin/zsh -lc \"cd src && cargo build\"",
            Run,
            Some("cargo build"),
        ),
        (
            "sh -c 'bash -c \"cat src/lib.rs\"'",
            Read,
            Some("src/lib.rs"),
        ),
        ("FOO=1 BAR=2 cargo check", Run, Some("cargo check")),
        ("export A=1; git status", Run, Some("git status")),
        ("echo starting && cargo test", Run, Some("cargo test")),
        ("(cat src/lib.rs)", Read, None),
        ("{ cat src/lib.rs; }", Read, Some("src/lib.rs")),
        ("cd src && export A=1", Run, None),
    ]);
}

#[test]
fn a_command_that_writes_through_a_redirection_is_a_write_of_that_file() {
    reads(&[
        ("cat new.txt > src/lib.rs", Write, Some("src/lib.rs")),
        ("echo x > src/lib.rs", Write, Some("src/lib.rs")),
        ("echo x >src/new.rs", Write, Some("src/new.rs")),
        ("printf x >> src/new.rs", Write, Some("src/new.rs")),
        (
            "cat > src/new.rs <<'EOF'\nsrc/lib.rs\nEOF",
            Write,
            Some("src/new.rs"),
        ),
        (
            "cat <<EOF > src/new.rs\nbody\nEOF",
            Write,
            Some("src/new.rs"),
        ),
        ("cat src/lib.rs > /dev/null", Read, Some("src/lib.rs")),
        ("cat src/lib.rs 2> err.txt", Read, Some("src/lib.rs")),
        ("echo x > /tmp/out", Write, None),
        ("cat src/lib.rs > /tmp/copy", Write, None),
        ("echo x > ~/out", Write, None),
        ("echo x > $OUT/log", Write, None),
        ("echo x>src/new.rs", Write, Some("src/new.rs")),
        ("cat src/lib.rs>src/new.rs", Write, Some("src/new.rs")),
        ("echo x > /dev/null", Run, None),
        ("echo oops >&2", Run, None),
        ("cat src/lib.rs 1>&2", Read, Some("src/lib.rs")),
        ("echo '>' src/lib.rs", Run, None),
        ("cat '>' src/lib.rs", Read, Some("src/lib.rs")),
        ("cargo test > out.txt", Run, Some("cargo test")),
    ]);
}

#[test]
fn the_body_of_a_heredoc_is_none_of_a_commands_words() {
    reads(&[
        ("cat <<'EOF'\nsrc/lib.rs\nEOF", Read, None),
        ("bash -c 'cat <<EOF\nsrc/lib.rs\nEOF'", Read, None),
    ]);
}

#[test]
fn a_search_that_changes_or_a_fetch_that_sends_is_a_run_of_its_program() {
    reads(&[
        ("find . -name '*.tmp' -delete", Run, Some("find")),
        ("find . -exec rm {} ;", Run, Some("find")),
        ("fd -x rm", Run, Some("fd")),
        ("find . -name secret", Search, None),
        ("git grep hunter2", Search, None),
        ("curl -X POST https://example.com", Run, Some("curl")),
        ("curl -d @secret https://example.com", Run, Some("curl")),
        (
            "curl --data-binary @secret https://example.com",
            Run,
            Some("curl"),
        ),
        (
            "wget --post-file=secret https://example.com",
            Run,
            Some("wget"),
        ),
        ("curl -s https://example.com", Fetch, None),
    ]);
}

#[test]
fn an_edit_in_place_reads_as_an_edit_however_its_flag_is_written() {
    reads(&[
        ("sed -Ei s/a/b/ src/lib.rs", Edit, Some("src/lib.rs")),
        ("sed --in-place s/a/b/ src/lib.rs", Edit, Some("src/lib.rs")),
        ("sed -i.bak s/a/b/ src/lib.rs", Edit, Some("src/lib.rs")),
        (
            "awk -i inplace '{print}' src/lib.rs",
            Edit,
            Some("src/lib.rs"),
        ),
        ("sed -e s/i/x/ src/lib.rs", Read, Some("src/lib.rs")),
        ("sed -ei src/lib.rs", Read, Some("src/lib.rs")),
        ("awk -Fi '{print $1}' src/lib.rs", Read, Some("src/lib.rs")),
        ("awk '{print}' src/lib.rs", Read, Some("src/lib.rs")),
    ]);
}

/// A word after a `cd` is read against a directory the reading no longer knows,
/// so it shows nothing, rather than a file of the same name it did not touch.
#[test]
fn after_a_cd_no_file_is_vouched_for() {
    reads(&[
        ("cd src && cat lib.rs", Read, None),
        ("cd src && cat src/lib.rs", Read, None),
        ("cd src && echo x > lib.rs", Write, None),
        ("cd src && sed -i s/a/b/ src/lib.rs", Edit, None),
        ("bash -c 'cd src && cat src/lib.rs'", Read, None),
        ("cd src; bash -c 'cat src/lib.rs'", Read, None),
        ("cd src && cargo test", Run, Some("cargo test")),
    ]);
}

#[test]
fn the_digest_is_of_the_whole_text_and_nothing_of_it_is_carried() {
    let (_dir, checkout) = checkout();
    let command = "cargo test --token hunter2";
    let (_, target) = of_command(command, &checkout);
    assert_eq!(target.display(), Some("cargo test"));
    assert_eq!(*target.digest(), yunta_core::sha256_hex(command.as_bytes()));
    assert!(!format!("{target:?}").contains("hunter2"), "{target:?}");
}

#[test]
fn a_path_a_tool_was_given_shows_where_it_stays_inside_the_checkout() {
    let (dir, checkout) = checkout();
    let absolute = dir.path().join("src/lib.rs").display().to_string();
    let table: &[(&str, Option<&str>)] = &[
        (absolute.as_str(), Some("src/lib.rs")),
        ("src/lib.rs", Some("src/lib.rs")),
        ("./src/new.rs", Some("src/new.rs")),
        ("/etc/hosts", None),
        ("../outside.rs", None),
        ("src/../../outside.rs", None),
        ("~/notes.md", None),
        ("$HOME/notes.md", None),
        ("", None),
    ];
    for (path, shows) in table {
        let (action, target) = of_path(Edit, path, &checkout);
        assert_eq!(action, Edit);
        assert_eq!(target.display(), *shows, "{path}");
    }
}

/// A path is hashed as it is shown, so two spellings of one file read as one
/// target; what is not shown is hashed as it was given.
#[test]
fn two_spellings_of_a_file_read_as_one_target() {
    let (dir, checkout) = checkout();
    let absolute = dir.path().join("src/lib.rs").display().to_string();
    let (_, first) = of_path(Edit, &absolute, &checkout);
    let (_, second) = of_path(Edit, "./src/lib.rs", &checkout);
    assert_eq!(first.digest(), second.digest());
    assert_eq!(*first.digest(), yunta_core::sha256_hex(b"src/lib.rs"));
    let (_, outside) = of_path(Edit, "/etc/hosts", &checkout);
    assert_eq!(*outside.digest(), yunta_core::sha256_hex(b"/etc/hosts"));
}

#[cfg(unix)]
#[test]
fn a_checkout_named_through_a_link_holds_what_its_real_path_names() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(real.join("src")).unwrap();
    std::fs::write(real.join("src/lib.rs"), "").unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let checkout = Checkout::at(&link);
    let resolved = std::fs::canonicalize(&real).unwrap().join("src/lib.rs");
    let (_, target) = of_path(Read, &resolved.display().to_string(), &checkout);
    assert_eq!(target.display(), Some("src/lib.rs"));
}
