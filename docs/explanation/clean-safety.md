# `gw clean` safety rules

`gw clean` is the only command that destroys anything. Its rules are built
around one principle: **fail closed**. A fact that could not be observed is
never treated as a reason to remove.

## The order of checks

For each worktree, `gw` finds the first applicable fact, in this order. Every
fact that protects work comes before every reason to destroy it.

1. **Harness worktree.** Owned by the agent harness. Skip.
2. **No branch.** A detached `HEAD` is not managed. Skip.
3. **Protected.** The default branch or the bare repository's `HEAD`. Keep.
4. **Prunable.** Git says the directory is gone. Prune the metadata; there is
   no checkout to lose.
5. **Unreadable `HEAD`.** Keep.
6. **Unpublished commits.** Some commit is reachable from no remote. Keep.
7. **Uncommitted changes.** Keep. An unreadable status also counts here.
8. **Merged PR.** Remove.
9. **Stale.** The last commit is older than the stale limit. Remove.
10. **Closed ticket.** Remove with `--closed`; otherwise recommend.
11. **No signal.** Keep.

When the closed-ticket fact applies but `--closed` was not given, `gw` prints
a recommendation and removes nothing. A recommendation is not a deletion, so
it does not need the restore payload.

## Why unreadable is not clean

`git status` can fail: a stale index lock, bad permissions, a broken worktree.
Empty output from a failed command looks exactly like a clean tree if the
state is a boolean. So cleanliness is a three-state value: `clean`, `dirty`,
`unknown`. `unknown` is treated as dirty.

The same applies to the last commit date. Activity with an unknown source is
never stale, so a clock or read failure cannot age a worktree into removal.

## What counts as published

A commit is published when it is reachable from any remote ref, not only from
`origin/<branch>`. `gw` builds that set once from `git rev-list --remotes`
and does a binary search for each tip. A commit reachable from a different
remote branch is still safe to keep.

## Protected from removal vs. protected from deletion

These are different sets on purpose:

- **Branch deletion** protects the default branch, the bare `HEAD`, and every
  branch checked out in a worktree. Deleting a ref that a checkout uses would
  strand it.
- **Worktree removal** protects only the default branch and the bare `HEAD`.

Conflating the two would make every worktree unremovable, because every
worktree's branch is, by definition, checked out.

## Admission before confirmation

Applying a plan requires an explicit opt-in:

- `--dry-run` always plans only
- otherwise `--yes` applies
- otherwise the plan applies only when stdout is a terminal

A piped or scripted run therefore stays a dry run unless it says `--yes`. The
default posture follows whether a human is present to watch.

## Refusal at execution time

The executor does not re-derive any decision. It executes the plan. It still
checks that each removal target is inside the managed worktrees directory
before handing the path to git. A path that is somehow outside is refused and
counted as a failure.

## Restoring a deletion

A delete decision carries the tip commit as a required field. The type cannot
be constructed without the data needed to restore the branch, and the command
prints `git branch <name> <tip>` for each deletion.

See [`gw clean` how-to](../how-to/clean-worktrees.md) for the commands.
