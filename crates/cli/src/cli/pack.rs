//! The subcommands of `yunta pack`, as the command line names them.

use clap::Subcommand;
use yunta_core::PackRef;

#[derive(Subcommand)]
pub(super) enum PackAction {
    /// Clones, vendors to `.yunta/packs/<publisher>/<name>/`, and locks a
    /// pack. `<source>` is a git URL or `host/publisher/name` shorthand,
    /// optionally suffixed `@<ref>` (tag, branch or commit-ish).
    Add {
        source: String,
        /// Confirms installing a pack that ships executors (executable
        /// code, not just declarative YAML), required whenever the
        /// pack's `declares.executors` is non-empty; review the audit
        /// this command prints first.
        #[arg(long)]
        yes: bool,
        /// Runs the pack's own test cases (`.yunta/tests/`, against the
        /// mock adapter) once it is installed. Without it nothing of the
        /// pack executes during `add`: the audit is read, not run.
        #[arg(long)]
        run_tests: bool,
    },
    /// Re-clones an installed pack at a new ref and re-vendors it.
    Update {
        /// `publisher/name`.
        pack: PackRef,
        r#ref: String,
        /// Confirms updating to a ref that declares executors, the same
        /// gate as `add`: a new ref is where new executable code first
        /// appears. `permissions.packs.executors: deny` refuses
        /// regardless; `allow` skips the confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Scaffolds a new pack at `./<name>`: a manifest, one verified
    /// workflow, a self-test config and case, and a README. Checks and
    /// tests it on creation, so the scaffold passes from the first command.
    New {
        /// `publisher/name`.
        pack: PackRef,
    },
    /// Removes a pack's vendored directory and its lock entry.
    Remove {
        /// `publisher/name`.
        pack: PackRef,
    },
    /// Lists every locked pack, verifying its vendored content against
    /// the lock.
    List,
    /// Full static inventory of an installed pack: every
    /// command, context source, per-node permission, required agent,
    /// mcp server, executor, and each workflow's full prompt text,
    /// plus whether the pack ships tests and whether they pass.
    /// `add` runs this automatically before vendoring; this is the
    /// on-demand form.
    Audit {
        /// `publisher/name`.
        pack: PackRef,
    },
}
