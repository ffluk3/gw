# User Manual

These pages explain *why* `gw` works the way it does, for people who use it.
They are not step-by-step instructions; the [how-to guides](../how-to/index.md)
cover the commands.

Read them in this order:

1. **[Why one checkout per branch](why-worktree-layout.md)** — the problem
   worktrees solve, why the repository is bare, and what you get that plain git
   does not.
2. **[Is it safe to delete?](clean-safety.md)** — the exact checks `gw clean`
   runs, and why an unknown answer always means "keep".

## Words you will see

| Term | Means |
| --- | --- |
| **layout** | One bare repository plus one checkout for each branch, in one directory. |
| **bare repository** | A copy of the repository with history but no files checked out. Every worktree shares it. |
| **worktree** | One checkout of one branch, in its own directory, with its own `HEAD`. |
| **repos root** | The directory where layouts live, `~/repos` by default (`GW_REPOS_ROOT`). |
| **provider** | A system `gw` asks about your work, such as JIRA for tickets or GitHub for pull requests. |
| **convention** | The rule that turns a branch name into a ticket or pull request lookup. |

## Working on `gw` itself?

The [Developer Manual](../developer/index.md) covers the decision model, the
branch-to-ticket mapping, discovery internals, and the crate layout.
