# Why a bare-repo worktree layout

A normal clone gives you one checkout and one branch at a time. To work on a
second branch, you switch. Switching means that you commit your work or stash
it. If you work on several branches in one day, switching costs time and loses
context.

Git worktrees correct this problem. Each worktree is a real checkout with its
own `HEAD`, and all worktrees share one object database. You change branches by
changing directory. Nothing is stashed.

The layout that `gw` manages is one common way to arrange this:

```text
<root>/
  .bare/               the shared repository
  worktrees/<branch>/  one checkout for each branch
```

## Why a bare repository

A normal clone has a `.git` directory and a working tree. If worktrees hang off
that clone, one worktree is the primary checkout and the other worktrees are
secondary. This asymmetry causes problems. The primary checkout occupies a
branch that the other worktrees cannot use, and its directory is beside the
worktree directory, not in it.

A bare repository has no working tree and no primary checkout. Each checkout is
a linked worktree, and each worktree looks and behaves the same way. There is
no special case.

## Why one directory for each branch

When you name the checkout after the branch, the mapping is predictable:

- the directory tells you which branch it holds
- `gw add feature/x` has an obvious destination
- a tool that asks "which branch is this?" can answer from the path

The tradeoff is that branch names with slashes make nested directories
(`worktrees/feature/x`). `gw` accepts this result, because it keeps
compatibility with existing checkouts.

## What `gw` adds

Git already supports all of this. `gw` adds:

- one command to open or reuse a worktree, with the local, remote, and new
  cases handled
- a status view for each worktree in one layout
- a safe cleanup pass that proves publication before it removes anything
- a way to address a layout from any directory, by root or by name

`gw` has no built-in host, repository, or ticket system.
