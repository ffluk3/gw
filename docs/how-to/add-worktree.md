# Open or reuse a worktree

Use this guide to get a checkout for a branch.

## Open a worktree and enter it

```bash
gw add PROJ-1234
```

With the shell integration sourced, `gw add` changes into the new worktree.
The `gw` shell function from that file performs the `cd`. Without the
integration, the binary prints the path and your shell stays where it is,
because no process can change the directory of its parent.

`--cd` says the same thing. It is useful only to override `GW_CD=0`.

## Open a worktree without changing directory

```bash
gw add PROJ-1234 --no-cd
```

The `--no-cd` flag wins over everything else for one call. To make staying put
the default, set `GW_CD=0` in your shell profile. To capture only the path:

```bash
worktree="$(gw add --print-path PROJ-1234)"
```

Do not name that variable `path` in zsh. zsh ties `path` to `PATH`, so the
assignment also replaces your command search path.

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
