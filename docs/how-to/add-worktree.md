# Open or reuse a worktree

Use this guide to get a checkout for a branch.

## Open a worktree and enter it

```bash
gwadd PROJ-1234
```

`gwadd` comes from the shell integration. It calls `gw add --print-path` and
then `cd`s into the result. A child process cannot change its parent's
directory, which is why the wrapper exists.

## Open a worktree without changing directory

```bash
gw add PROJ-1234
```

The command prints the path. To capture only the path:

```bash
path="$(gw add --print-path PROJ-1234)"
```

## Understand the four cases

`gw add` picks a strategy from what already exists:

| State | What `gw` does |
| --- | --- |
| A worktree checks out the branch | Reuses it |
| A local branch exists | Moves it to `origin/<branch>`, then checks it out |
| Only a remote branch exists | Makes a local branch that tracks it |
| The branch does not exist | Makes a new branch from `HEAD` |

## Skip the fetch

`gw add` fetches from origin first, so the new branch starts from current
refs. To skip the fetch:

```bash
gw add PROJ-1234 --no-fetch
```

If the fetch fails, `gw` prints a warning and continues with cached refs.

## Know where the checkout goes

The directory follows the branch name. `feature/x` becomes
`worktrees/feature/x`:

```text
<root>/worktrees/feature/x/
```

## If the branch is checked out elsewhere

A branch can have only one checkout. If `gw` cannot fast-forward a local
branch because another worktree holds it, move or remove that worktree first.
The error message names the branch.
