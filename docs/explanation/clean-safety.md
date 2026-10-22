# `gw clean` safety rules

`gw clean` is the only command that destroys anything. Its rules have one
principle: **fail closed**. A fact that `gw` could not observe is never a
reason to remove.

## The order of checks

For each worktree, `gw` finds the first applicable fact, in this order. Each
fact that protects work comes before each reason to destroy work.

1. **Harness worktree.** The agent harness owns it. Skip.
2. **No branch.** `gw` does not manage a detached `HEAD`. Skip.
3. **Protected.** The default branch or the `HEAD` of the bare repository.
   Keep.
4. **Prunable.** Git reports that the directory is gone. Prune the metadata.
   There is no checkout to lose.
5. **Unreadable `HEAD`.** Keep.
6. **Unpublished commits.** Some commit is not reachable from a remote. Keep.
7. **Uncommitted changes.** Keep. An unreadable status also counts here.
8. **Merged change.** Remove. Only an identity-strength observation counts,
   and only when no identity-strength observation for the same signal is
   `unknown`.
9. **Stale.** The last commit is older than the stale limit. Remove.
10. **Closed ticket.** Remove with `--closed`. Without `--closed`,
    recommend. The same rules on strength and disagreement apply.
11. **No signal.** Keep.

If the closed-ticket fact applies but you did not give `--closed`, `gw` prints
a recommendation and removes nothing. A recommendation is not a deletion, so it
does not need the restore data.

## Why a contested or weak fact is not evidence

A branch can be associated with a ticket or a change by more than one rule, and
a provider can fail. Two rules apply, and both are the fail-closed principle
applied to provider facts:

- **Only identity-strength evidence may destroy.** A rule that merely finds a
  ticket mentioned in text proves a topic, not ownership.
- **Disagreement is not evidence.** If one provider reports a merged change and
  another cannot answer, the signal is unusable. The worktree falls through to
  the weaker reasons, which still protect unpublished commits and dirty trees.

The second rule matters because a failed lookup must never be read as "not
merged". `unknown` is a value, and it blocks the verdict that it would have
justified.

## Why unreadable is not clean

`git status` can fail. A stale index lock, bad permissions, or a broken
worktree can cause the failure. The empty output of a failed command looks
exactly like a clean tree when the state is a boolean. Thus cleanliness has
three states: `clean`, `dirty`, and `unknown`. `gw` treats `unknown` as dirty.

The same rule applies to the last commit date. Activity from an unknown source
is never stale, so a clock error or a read error cannot make a worktree old
enough for removal.

## What counts as published

A commit is published when it is reachable from any remote ref, not only from
`origin/<branch>`. `gw` builds that set one time from `git rev-list --remotes`
and does a binary search for each tip. A commit that is reachable from a
different remote branch is still safe to keep.

## Protected from removal compared to protected from deletion

These are different sets, by design:

- **Branch deletion** protects the default branch, the bare `HEAD`, and each
  branch that a worktree checks out. A deleted ref that a checkout uses leaves
  that checkout with no branch.
- **Worktree removal** protects only the default branch and the bare `HEAD`.

If `gw` uses the same set for both operations, `gw` cannot remove a worktree,
because each worktree checks out its own branch.

## Admission before confirmation

To apply a plan, you need an explicit opt-in:

- `--dry-run` always plans only
- otherwise, `--yes` applies
- otherwise, `--json` plans only
- otherwise, the plan applies only when stdout is a terminal

A piped run or a scripted run thus stays a dry run unless it gives `--yes`. The
default follows whether a human is present to watch the run.

`--json` counts as "no human present", even when stdout is a terminal. An agent
harness can allocate a pty, and a terminal is not proof that someone watches.
Machine output is a request for a report, not a confirmation.

## Refusal at execution time

The executor does not derive a decision again. It runs the plan. It still makes
sure that each removal target is inside the managed worktrees directory before
it gives the path to git. If a path is outside the directory, `gw` refuses the
path and counts it as a failure.

## Restoring a deletion

A delete decision has the tip commit as a required field. `gw` cannot construct
the type without the data that it needs to restore the branch. The command
prints `git branch <name> <tip>` for each deletion.

See the [`gw clean` how-to](../how-to/clean-worktrees.md) for the commands.
