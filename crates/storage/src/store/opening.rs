//! Opening the event log: the lock that lets one opener at a time set up a
//! log, and the journal mode and schema each handle is opened with.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use super::{cause, Storage, SCHEMA_V1};
use crate::error::{Result, StorageError};

/// Holds the opening of the log at `log` against every other process and
/// thread: a fresh database switches to WAL under a lock SQLite does not
/// wait for, so two openings of a new log at once would see `database is
/// locked`. Released when the returned file is dropped, once the journal
/// mode and the schema are set.
fn hold_the_opening(log: &Path) -> std::io::Result<std::fs::File> {
    let mut beside = log.as_os_str().to_owned();
    beside.push(".opening");
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(PathBuf::from(beside))?;
    guard.lock()?;
    Ok(guard)
}

impl Storage {
    /// Opens (creating if needed) the event log at `path` in WAL mode,
    /// running the embedded schema migration.
    pub fn open(path: &Path) -> Result<Self> {
        let open_err = |error| StorageError::Open {
            path: path.to_path_buf(),
            source: cause(error),
        };

        let conn = Connection::open(path).map_err(open_err)?;
        // WAL admits exactly one writer at a time; a second handle onto
        // the same database (a status/follower reader, a per-session
        // run-tools listener) must wait for a busy writer instead of
        // surfacing SQLITE_BUSY as a spurious append failure. Writes
        // here are all sub-millisecond appends — 5s of patience means
        // something is truly wedged, not busy. Set before the journal
        // pragma so even that first statement waits rather than fails
        // when another connection happens to be mid-write.
        // Note the timeout alone is NOT enough for `append` — see the
        // BEGIN IMMEDIATE note there.
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(open_err)?;
        set_up(&conn, path)?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }
}

/// Puts the log at `path` in WAL mode with its schema. A log already in
/// WAL is set up without the lock: the switch is the one step SQLite does
/// not wait for, and it is done. A new log is switched under the lock, one
/// opener at a time.
fn set_up(conn: &Connection, path: &Path) -> Result<()> {
    let open_err = |error| StorageError::Open {
        path: path.to_path_buf(),
        source: cause(error),
    };
    let held = |error: std::io::Error| StorageError::Open {
        path: path.to_path_buf(),
        source: Box::new(error),
    };
    let mut opening = None;
    if !in_wal(conn).map_err(open_err)? {
        opening = Some(hold_the_opening(path).map_err(held)?);
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(open_err)?;
    }
    conn.execute_batch(SCHEMA_V1).map_err(open_err)?;

    // A database created before the chain existed has the table but not
    // the column — added here once, under the lock, with old rows left
    // NULL (visible to `verify_chain` as a break, never silently
    // backfilled with hashes nothing ever wrote).
    if !has_hash_column(conn).map_err(open_err)? {
        if opening.is_none() {
            opening = Some(hold_the_opening(path).map_err(held)?);
        }
        if !has_hash_column(conn).map_err(open_err)? {
            conn.execute("ALTER TABLE events ADD COLUMN event_hash TEXT", [])
                .map_err(open_err)?;
        }
    }
    drop(opening);
    Ok(())
}

/// Whether the log's journal is already WAL: read, never switched.
fn in_wal(conn: &Connection) -> rusqlite::Result<bool> {
    let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    Ok(mode.eq_ignore_ascii_case("wal"))
}

/// Whether the events table carries the chain's hash column.
fn has_hash_column(conn: &Connection) -> rusqlite::Result<bool> {
    conn.prepare("SELECT 1 FROM pragma_table_info('events') WHERE name = 'event_hash'")
        .and_then(|mut stmt| stmt.exists([]))
}
