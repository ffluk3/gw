# Open or reuse a worktree

Use this guide to get a checkout for a branch.

## Open a worktree and enter it

```bash
gwadd PROJ-1234
```

`gwadd` comes from the shell integration. It runs `gw add --cd`, and the `gw`
shell function `cd`s into the result. A child process cannot change its
parent's directory, which is why the wrapper exists.

The same call without the `gwadd` shorthand:

```bash
gw add --cd PROJ-1234
```

`--cd` only works when the shell integration is sourced. Without it, the
binary prints the path and your shell stays where it is, because no process
can change its parent's directory.

To make every `gw add` navigate, set the default once:

```bash
export GW_CD=1
```

Then `gw add PROJ-1234` enters the worktree. Use `--no-cd` for one call that
should stay put.

## Open a worktree without changing directory

```bash
gw add PROJ-1234
```

The command prints the path and your shell stays where it is. With `GW_CD=1`
set, add `--no-cd` to that call. To capture only the path:

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
