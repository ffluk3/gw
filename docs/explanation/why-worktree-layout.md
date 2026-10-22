# Why one checkout per branch

Most people meet `git` as a single working copy. That copy is checked out on one
branch at a time, so working on a second ticket means switching. Switching costs
something every time:

- your unfinished edits need somewhere to go — a stash you might forget, or a
  "wip" commit you have to clean up later
- your editor reloads every file, and your tests, dev server, and build cache
  now point at a different branch
- the work you left behind becomes invisible. Nothing on screen says "there are
  three other things half-done."

If you switch a few times a day, that cost adds up. It is also when mistakes
happen: a stash popped onto the wrong branch, or a fix committed to the ticket
you were supposed to be reviewing.

## Worktrees fix the switching

A git **worktree** is a real checkout with its own `HEAD`. Several worktrees can
belong to one repository and share a single object database, so the history is
stored once and each checkout borrows from it.

The consequence is simple: to change branches, you change directory. Nothing is
stashed, nothing reloads, and everything you had open is still open.

`gw` manages one common way to arrange that:

```text
<root>/
  .bare/               the shared repository
  worktrees/<branch>/  one checkout for each branch
```

The first directory, `.bare/`, is a **bare repository**: history with no files
checked out. It is the shared store every worktree reads from. Each
`worktrees/<branch>/` is an ordinary checkout — `git status`, `git diff`, and
your tools work there exactly as they always have.

## Why the repository is bare

A normal clone has a `.git` directory and a working tree, and its first worktree
is implicitly the "main" one. That asymmetry is annoying in practice. The
primary checkout occupies a branch, so no other worktree can use that branch.
Its directory also sits beside the worktree directory rather than inside it, so
the layout looks like a special case.

A bare repository has no working tree and no primary checkout. Every worktree is
a linked worktree of the same repository, and every one behaves identically.
There is no favorite child and no exception to remember.

## Why one directory per branch

Naming each checkout after its branch makes the layout predictable:

- the directory name tells you which branch is inside it
- `gw add feature/x` has an obvious destination
- a tool that needs to know "what branch is this?" can read it from the path

The tradeoff is that branch names with slashes become nested directories
(`worktrees/feature/x`). `gw` accepts that, because it keeps compatibility with
checkouts people already have.

## What `gw` adds

All of the above is plain git. `gw` handles the work you would otherwise repeat
by hand:

- **one command** to open or reuse a worktree, with the existing, remote, and
  new-branch cases handled
- **one status view** across every worktree in the layout
- **a cleanup pass** that proves work is published before it removes anything
- **stable addressing** so a command works from any directory, by root or name

For the concrete difference from plain `git worktree`, including what `gw`
deliberately does not do, see the [comparison on the home page](../index.md).

## Next

- [Is it safe to delete?](clean-safety.md) — what `clean` checks, and in what
  order.
- [Developer Manual](../developer/index.md) — how the decision logic is built.
