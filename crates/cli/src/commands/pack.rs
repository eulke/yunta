//! `yunta pack add/remove/list/update` (RFC-0002 §4, T11.2): the
//! command layer — argument handling and printing — over
//! `crate::pack`'s git/filesystem mechanics. `add` also gates on
//! executors (T11.5/§6.3): a pack that declares any is refused unless
//! `--yes` confirms it, after the audit above has shown exactly what
//! they are.

use std::process::ExitCode;

use yunta_core::PackLockEntry;
use yunta_engine::audit_pack;

use super::pack_audit::{print_report, run_pack_tests};
use crate::pack::{
    clone_pack, clone_url, current_branch, hash_tree, head_commit, load_lock, lock_path,
    packs_root, read_manifest, save_lock, split_source_and_ref, vendor_dir, vendor_tree,
};

pub async fn add(source: &str, confirmed_executors: bool) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let (source_part, ref_arg) = split_source_and_ref(source);
    let url = clone_url(source_part);

    let clone_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: could not create a temp directory to clone into: {e}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = clone_pack(&url, ref_arg, clone_dir.path()).await {
        eprintln!("error: could not clone `{url}`: {e}");
        return ExitCode::FAILURE;
    }

    let manifest = match read_manifest(clone_dir.path()) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let dest = vendor_dir(&cwd, &manifest.publisher, &manifest.name);
    if dest.exists() {
        eprintln!(
            "error: `{}/{}` is already installed at {} — use `yunta pack update` to change its \
             ref, or `yunta pack remove` first",
            manifest.publisher,
            manifest.name,
            dest.display()
        );
        return ExitCode::FAILURE;
    }

    // §6: "corre el audit... nada ejecuta hasta que el humano vio el
    // inventario" — shown before anything is vendored, let alone run.
    let audit = audit_pack(clone_dir.path(), manifest.clone());
    print_report(&audit);
    let tests = run_pack_tests(clone_dir.path()).await;
    if !tests.has_tests {
        println!("\ntests: none shipped");
    } else {
        println!("\ntests: {} case(s), {} failed", tests.total, tests.failed);
        for line in &tests.failures {
            println!("  {line}");
        }
    }
    println!();

    // T11.5/§6.3: executors are code, not declarative YAML — installing
    // them needs an explicit, separate confirmation on top of the audit
    // above, not just a note nobody has to acknowledge.
    if !manifest.declares.executors.is_empty() && !confirmed_executors {
        eprintln!(
            "error: this pack declares {} executor(s) — executable code, not just declarative \
             YAML. Review the inventory above, then re-run with `--yes` to confirm installing \
             it.",
            manifest.declares.executors.len()
        );
        return ExitCode::FAILURE;
    }

    let commit = match head_commit(clone_dir.path()).await {
        Ok(commit) => commit,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let resolved_ref = match ref_arg {
        Some(ref_arg) => ref_arg.to_string(),
        None => match current_branch(clone_dir.path()).await {
            Ok(branch) => branch,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
    };

    if let Err(e) = vendor_tree(clone_dir.path(), &dest) {
        eprintln!(
            "error: could not vendor the pack into {}: {e}",
            dest.display()
        );
        return ExitCode::FAILURE;
    }
    let content_hash = match hash_tree(&dest) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key = yunta_core::PackLock::key(&manifest.publisher, &manifest.name);
    lock.packs.insert(
        key,
        PackLockEntry {
            publisher: manifest.publisher.clone(),
            name: manifest.name.clone(),
            source: url,
            r#ref: resolved_ref,
            commit: commit.clone(),
            content_hash,
        },
    );
    if let Err(e) = save_lock(&cwd, &lock) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "installed {}/{} @ {} ({}) -> {}",
        manifest.publisher,
        manifest.name,
        manifest.version,
        &commit[..commit.len().min(12)],
        dest.display()
    );
    ExitCode::SUCCESS
}

pub async fn update(publisher_name: &str, new_ref: &str) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some((publisher, name)) = publisher_name.split_once('/') else {
        eprintln!("error: `{publisher_name}` isn't `publisher/name`");
        return ExitCode::FAILURE;
    };

    let mut lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key = yunta_core::PackLock::key(publisher, name);
    let Some(entry) = lock.packs.get(&key).cloned() else {
        eprintln!(
            "error: `{publisher_name}` isn't installed — `yunta pack add` it first, not update"
        );
        return ExitCode::FAILURE;
    };

    let clone_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("error: could not create a temp directory to clone into: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = clone_pack(&entry.source, Some(new_ref), clone_dir.path()).await {
        eprintln!(
            "error: could not clone `{}` at `{new_ref}`: {e}",
            entry.source
        );
        return ExitCode::FAILURE;
    }
    let manifest = match read_manifest(clone_dir.path()) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if manifest.publisher != publisher || manifest.name != name {
        eprintln!(
            "error: `{new_ref}` of `{}` identifies itself as `{}/{}`, not `{publisher_name}` — \
             refusing to update a pack into a different one",
            entry.source, manifest.publisher, manifest.name
        );
        return ExitCode::FAILURE;
    }

    let commit = match head_commit(clone_dir.path()).await {
        Ok(commit) => commit,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let dest = vendor_dir(&cwd, publisher, name);
    if let Err(e) = std::fs::remove_dir_all(&dest) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "error: could not clear {} before revendoring: {e}",
                dest.display()
            );
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = vendor_tree(clone_dir.path(), &dest) {
        eprintln!(
            "error: could not vendor the pack into {}: {e}",
            dest.display()
        );
        return ExitCode::FAILURE;
    }
    let content_hash = match hash_tree(&dest) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    lock.packs.insert(
        key,
        PackLockEntry {
            publisher: publisher.to_string(),
            name: name.to_string(),
            source: entry.source,
            r#ref: new_ref.to_string(),
            commit: commit.clone(),
            content_hash,
        },
    );
    if let Err(e) = save_lock(&cwd, &lock) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "updated {publisher}/{name} -> {new_ref} ({})",
        &commit[..commit.len().min(12)]
    );
    ExitCode::SUCCESS
}

pub fn remove(publisher_name: &str) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some((publisher, name)) = publisher_name.split_once('/') else {
        eprintln!("error: `{publisher_name}` isn't `publisher/name`");
        return ExitCode::FAILURE;
    };

    let mut lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let key = yunta_core::PackLock::key(publisher, name);
    if lock.packs.remove(&key).is_none() {
        eprintln!("error: `{publisher_name}` isn't installed");
        return ExitCode::FAILURE;
    }

    let dest = vendor_dir(&cwd, publisher, name);
    if let Err(e) = std::fs::remove_dir_all(&dest) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("error: could not remove {}: {e}", dest.display());
            return ExitCode::FAILURE;
        }
    }
    if let Err(e) = save_lock(&cwd, &lock) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!("removed {publisher_name}");
    ExitCode::SUCCESS
}

pub fn list() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot determine the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let lock = match load_lock(&cwd) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if lock.packs.is_empty() {
        println!("no packs installed under {}", packs_root(&cwd).display());
        return ExitCode::SUCCESS;
    }

    // §4's own "instalación offline reproducible... verificando contra
    // el lock": re-hash what's actually vendored on disk and say so
    // when it no longer matches what the lock recorded, rather than
    // just trusting the lock's own numbers back at the user.
    for (key, entry) in &lock.packs {
        let dest = vendor_dir(&cwd, &entry.publisher, &entry.name);
        let status = match hash_tree(&dest) {
            Ok(hash) if hash == entry.content_hash => "ok".to_string(),
            Ok(_) => "MODIFIED (vendored content no longer matches the lock)".to_string(),
            Err(_) => "MISSING (vendored directory not found)".to_string(),
        };
        println!(
            "{key} @ {} ({}) — {status}",
            entry.r#ref,
            &entry.commit[..entry.commit.len().min(12)]
        );
    }
    println!("lock: {}", lock_path(&cwd).display());
    ExitCode::SUCCESS
}
