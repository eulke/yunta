---
number: D257
title: "Git has one owner, and each call names its answer, its lock and its failure"
status: revised
revises: []
revised_by: [D267]
---

# D257 — Git has one owner, and each call names its answer, its lock and its failure

*(Revised by D267: the purity test named in item 1 no longer exists. A spawn of git
outside `engine/src/git/` is refused by the `git_command_outside_git_rs` count of the
smells ratchet, which holds it at zero. The decision itself is unchanged.)*

## Context

Git ran from dozens of call sites, through several helpers, and each one
decided for itself what a git that did not answer meant. `success` read any
non-zero exit as "no", so a `cat-file` that could not run and a file that
was absent looked alike, and a call stopped by Ctrl-C read as "no remote" or
"not an ancestor". A rebase that git refused for a dirty tree came back as a
conflict with no paths in it. A wait for the worktree-mutation lock lasted
thirty seconds whatever the run's cancellation said. Checkouts were removed,
and branches deleted, outside that lock, and a checkout that a unit still
kept could be cleared when its `.git` pointer was broken.

## Decision

1. **One owner.** `engine/src/git/` is the only code that starts git. Every
   call goes through one of its functions: `read` and `read_bytes` for
   output; `ask`, given the exits that mean yes and no, for a question;
   `verify` for a commit that may be absent; `change` and `attempt` for a
   mutation, with its `Effect`; `change_held` and `attempt_held` for a caller
   that holds the mutation lock already; `tree_state` for a checkout's
   status; `common_dir` and `mutation_lock`. A spawn anywhere else is refused
   by the purity test and counted by the smells ratchet.
2. **Exits name the answer.** A question, and a mutation git may decline,
   says which exit means yes or done and which means no or declined, as
   `Exits { yes, no }`. Any other exit is an error. A git that did not answer
   is a `GitError`: its invocation, and a `Cause` that is a stop by the token
   or the timeout, a spawn failure with its `io::Error`, an exit with git's
   own stderr, a drain that stopped reading, or a lock that was not got. Its
   text is composed in one place.
3. **The effect says whether the lock is taken.** `Metadata`, which adds,
   removes and prunes checkouts and deletes branches, takes
   `yunta-worktree.lock`. `Objects`, `Refs` and `Checkout` do not: a
   checkout's safety is its caller's hold on the slot, not the call's. An
   `update-ref` is a `Refs` call and stays without the lock, as a permanent
   rule: a ref write changes no checkout, so it has nothing to wait for.
4. **A stopped call is never an answer.** A stop by the token is
   `Cause::Stopped(Cancelled)`. A wait for a held lock that the token ends is
   a `LockError::Cancelled`; a metadata call that waited carries it as
   `Cause::Lock`, so `GitError::cancelled` reports it too. Neither is read as
   "no remote", "not an ancestor", "clean" or "no checkout". Output that the
   drain cut short is `Cause::Truncated`, not a complete answer.
5. **Which git runs.** The program is the first `git` on the `PATH` a call
   searches. It is looked up once for each distinct `PATH` value in a
   process, so a stub placed first on the `PATH` still runs. On macOS the
   system `/usr/bin/git` is a shim that resolves the real git on every call;
   when `xcrun --find git` names one, that one runs, and an `xcrun` that
   fails leaves the `PATH` program in use. The cost is one `xcrun` spawn for
   each `PATH` a process searches whose first git is the shim; every later
   call runs the resolved binary directly. An earlier measurement put one git
   call at 11.6 ms through the shim and at 4.9 ms through the Command Line
   Tools binary.
6. **The lock classes.** M is `yunta-worktree.lock` in the common git
   directory. A live holder is waited on for at most thirty seconds, the
   token ends the wait, and a dead holder is taken over. S is a checkout's
   `slot-N.lock`: a dead holder whose agents still work keeps it, and the
   file is not touched. U is the lock of `none` isolation, `yunta-none.lock`,
   which takes over a dead holder. Every lock is released by its owner alone:
   a release removes the file only while it still names that owner.

## Rationale

A git failure is information about the repository, and a caller that
decides on a verdict has to see whether git answered at all. One door keeps
that rule in one place. The exits are named at each call because the same
exit means different things to different commands: `merge-base --is-ancestor`
exits 1 for a commit that is not an ancestor and 128 for a commit the
repository lacks, while `rev-parse --verify --quiet` exits 1 for the commit it
lacks.

The lock follows the effect, not the caller. Only metadata changes can race
another process's `worktree add`; the other calls protect nothing a second
process could race, so taking the lock for them would only add waits.

## Rejected alternatives

**A lock on every git call.** Ref writes and reads would queue behind
checkout creation for as long as a waiter's patience lasts, and a cancelled
run would wait with them. A ref write changes no checkout, so the wait would
buy nothing.

**Exit codes read by each caller.** That is the shape this decision
replaces, and it is how a dirty tree read as a conflict.

## Consequences

- A rebase that git refuses, such as a dirty tree or an `index.lock`, fails
  its step with git's own message. It is no longer a criteria finding that
  reads `git rebase:` with nothing after it. This changes the outcome of new
  runs only; logs already written keep their findings.
- A run cancelled while it waits for another process's `worktree add` returns
  at once, not after thirty seconds.
- `gc` takes the mutation lock for each checkout it removes: the lock of the
  repository the checkout names, whichever project `gc` runs in.
- A checkout left by an older layout with uncommitted work is kept at the end
  of a run. `gc` removes only the clean ones.
- A dead holder whose agents still work keeps its checkout, and its lock file
  is left as it is until none of those agents is running.
