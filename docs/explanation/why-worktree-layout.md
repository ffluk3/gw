# Why a bare-repo worktree layout

A normal clone gives you one checkout and one branch at a time. To work on a
second branch you switch, and switching means either committing or stashing
your work. When you work on several branches a day, that switching costs time
and loses context.

Git worktrees solve this. Each worktree is a real checkout with its own
`HEAD`, sharing one object database. You change branches by changing
directory. Nothing is stashed.

The layout `gw` manages is one common way to arrange this:

```text
<root>/
  .bare/               the shared repository
  worktrees/<branch>/  one checkout for each branch
```

## Why a bare repository

A normal clone has a `.git` directory and a working tree. If worktrees hang off
that clone, one of them is the primary checkout and the rest are secondary.
That asymmetry creates problems: the primary checkout occupies a branch the
others cannot use, and its directory sits beside the worktree directory rather
than in it.

A bare repository has no working tree and no primary checkout. Every checkout
is a linked worktree, and they all look and behave the same. There is no
special case.

## Why one directory per branch

Naming the checkout after the branch makes the mapping predictable:

- the directory tells you which branch it holds
- `gw add feature/x` has an obvious destination
- tools that ask "which branch is this?" can answer from the path

The tradeoff is that branch names with slashes create nested directories
(`worktrees/feature/x`). `gw` accepts that, because it preserves compatibility
with existing checkouts.

## What `gw` adds

Git already supports all of this. `gw` adds:

- one command to open or reuse a worktree, with the local/remote/new cases
  handled
- a status view across every worktree in one layout
- a safe cleanup pass that proves work is published before removing anything
- a way to address a layout from anywhere, by root or by name

Nothing about a particular host, repository, or ticket system is built in.
