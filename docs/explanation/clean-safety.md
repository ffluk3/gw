# `gw clean` safety rules

`gw clean` is the only command that destroys anything. It would rather keep your
work than save you a folder, so its rules **fail closed**: a fact that `gw`
could not observe is never a reason to remove.

## The order of checks

For each worktree, `gw` finds the first applicable fact in the order below.
Every fact that protects work is tested before any reason to destroy work.
That order is the policy, not an implementation detail.

| # | Check | Verdict | Why |
| --- | --- | --- | --- |
| 1 | Harness worktree | Skip | An agent harness owns it. |
| 2 | No branch (detached `HEAD`) | Skip | `gw` does not manage detached `HEAD`. |
| 3 | Protected: the default branch, or the bare `HEAD` | Keep | Never destroy the repository's main line. |
| 4 | Prunable: git reports the directory is gone | Prune the metadata | No checkout exists to lose. |
| 5 | Unreadable `HEAD` | Keep | An unknown state is not an empty state. |
| 6 | Unpublished commits | Keep | Some commit is reachable from no remote. |
| 7 | Uncommitted changes, or an unreadable status | Keep | The work exists only here. |
| 8 | Merged change | Remove | Only an identity-strength observation counts, and only when no identity-strength observation for the same signal is `unknown`. |
| 9 | Stale: last commit older than the stale limit | Remove | A proxy for "abandoned". The limit comes from `--stale-days`/`GW_STALE_DAYS`, then `[clean] stale_after`, then 7 days. |
| 10 | Closed ticket | Remove with `--closed`; otherwise recommend | The same strength and disagreement rules apply. |
| 11 | No signal | Keep | Absence of evidence is not evidence. |

If the closed-ticket fact applies but you did not give `--closed`, `gw` prints a
recommendation and removes nothing. A recommendation destroys nothing, so it
does not need the restore data.

## Why a weak or contested fact is not evidence

A branch can be associated with a ticket or a change by more than one rule, and
a provider can fail. Both cases take the fail-closed principle one level up:

- **Only identity-strength evidence may destroy.** A rule that merely finds a
  ticket key mentioned in text proves a topic, not ownership.
- **Disagreement is not evidence.** If one provider reports a merged change and
  another cannot answer, the signal is unusable. The worktree falls through to
  the weaker reasons, which still protect unpublished commits and dirty trees.

The second rule matters because a failed lookup must never be read as "not
merged". `unknown` is a value, and it blocks the verdict it would otherwise have
justified.

## Why unreadable is not clean

`git status` can fail: a stale index lock, bad permissions, or a broken worktree.
The empty output of a failed command looks exactly like a clean tree if
cleanliness is a boolean. So cleanliness has three states: `clean`, `dirty`, and
`unknown`, and `gw` treats `unknown` as dirty.

The same rule applies to the last commit date. Activity from an unknown source
is never stale, so a clock or read error cannot make a worktree old enough to
remove.

## Why a configured threshold must fail loudly

`[clean] stale_after` is validated when the file loads. A value that `gw` cannot
read stops the command with exit code 2.

The alternative is worse than it looks. If an unreadable duration fell back to
the default, `gw` would plan against a limit nobody asked for. A team that wrote
`"5w"` and got 7 days would remove a month of worktrees. Failing closed means a
stale limit is never silently narrower than the one in the file.

## What counts as published

A commit is published when it is reachable from any remote ref, not only from
`origin/<branch>`. `gw` builds that set once from `git rev-list --remotes` and
does a binary search for each tip. A commit reachable from a different remote
branch is still safe to keep.

## Protected from removal compared to protected from deletion

These are different sets, by design:

- **Branch deletion** protects the default branch, the bare `HEAD`, and each
  branch that a worktree checks out. Deleting a ref that a checkout uses leaves
  that checkout with no branch.
- **Worktree removal** protects only the default branch and the bare `HEAD`.

If `gw` used the same set for both operations, it could remove no worktree at
all, because each worktree checks out its own branch.

## Admission before confirmation

To apply a plan, you need an explicit opt-in:

- `--dry-run` always plans only
- otherwise, `--yes` applies
- otherwise, `--json` plans only
- otherwise, the plan applies only when stdout is a terminal

A piped or scripted run therefore stays a dry run unless it gives `--yes`. The
default follows whether a human is present to watch the run.

`--json` counts as "no human present", even when stdout is a terminal. An agent
harness can allocate a pty, and a terminal is not proof that someone is
watching. Machine output is a request for a report, not a confirmation.

## Refusal at execution time

The executor does not derive a decision again. It runs the plan. It still makes
sure each removal target is inside the managed worktrees directory before it
gives the path to git. If a path is outside that directory, `gw` refuses it and
counts it as a failure.

## Restoring a deletion

A delete decision carries the tip commit as a required field. `gw` cannot
construct the type without the data needed to restore the branch. The command
prints `git branch <name> <tip>` for each deletion.

See the [`gw clean` how-to](../how-to/clean-worktrees.md) for the commands.
