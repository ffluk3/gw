# Open or reuse a worktree

Use this guide to get a checkout for a branch.

## Open a worktree and enter it

```bash
gwadd PROJ-1234
```

`gwadd` comes from the shell integration. It runs `gw add --cd`, and then the
`gw` shell function changes into the result. A child process cannot change the
directory of its parent, which is why the wrapper exists.

This is the same call without the `gwadd` shorthand:

```bash
gw add --cd PROJ-1234
```

`--cd` works only when the shell integration is sourced. Without it, the binary
prints the path and your shell stays where it is, because no process can change
the directory of its parent.

To make each `gw add` change directory, set the default one time:

```bash
export GW_CD=1
```

Then `gw add PROJ-1234` enters the worktree. Use `--no-cd` for one call that
must stay in the current directory.

## Open a worktree without changing directory

```bash
gw add PROJ-1234
```

The command prints the path and your shell stays where it is. If `GW_CD=1` is
set, add `--no-cd` to that call. To capture only the path:

```bash
path="$(gw add --print-path PROJ-1234)"
```

## Understand the four cases

`gw add` selects a strategy from the objects that already exist:

| State | What `gw` does |
| --- | --- |
| A worktree checks out the branch | Reuses the worktree |
| A local branch exists | Moves it to `origin/<branch>`, then checks it out |
| Only a remote branch exists | Makes a local branch that tracks the remote branch |
| The branch does not exist | Makes a new branch from `HEAD` |

## Skip the fetch

`gw add` fetches from origin first, so the new branch starts from current refs.
To skip the fetch:

```bash
gw add PROJ-1234 --no-fetch
```

If the fetch fails, `gw` prints a warning and continues with cached refs.

## Find the checkout directory

The directory follows the branch name. `feature/x` becomes
`worktrees/feature/x`:

```text
<root>/worktrees/feature/x/
```

## If the branch is checked out in another worktree

A branch can have only one checkout. If `gw` cannot fast-forward a local branch
because another worktree holds it, move or remove that worktree first. The
error message gives the name of the branch.
