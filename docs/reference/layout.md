# Layout on disk

## The convention

```text
<root>/
  .bare/               the bare repository
  worktrees/<branch>/  one checkout per branch
```

Branch slashes become directory levels: `feature/x` lives at
`worktrees/feature/x`.

## The alternate worktrees directory

An existing checkout may use `.bare.worktrees/` beside `worktrees/` instead.
`gw` uses whichever directory already exists. It does not create a second
directory beside the real one.

When `gw` loads a layout, it also reads `git worktree list` and adopts the
common parent of the linked worktrees if that differs from the convention.
Only a directory that is a direct child of the layout root and that contains
at least one linked worktree is accepted.

## The repos root

```text
<repos>/
  <owner>/
    <name>/          a layout root
      .bare/
      worktrees/
```

Both nested (`owner/name`) and flat (`name`) layouts are discovered. The repos
root is `GW_REPOS_ROOT`, or `~/repos` when unset.

## How `gw` recognizes a layout

A directory is a layout root when `<dir>/.bare/HEAD` is a file. Nothing else is
required, and no marker file is written by `gw`.

## Discovery from the current directory

`gw` accepts these starting points:

1. a linked worktree, including a subdirectory of one
2. the bare repository itself, `<root>/.bare/`
3. the layout root, `<root>/`
4. a subdirectory of the layout root that git can resolve

Anywhere else is an error. A normal clone, with a `.git` directory in the
project, is also an error, because git's common directory is not `.bare`.

## Agent harness worktrees

Paths containing `/.claude/worktrees/` are harness-owned. `gw clean` skips
them and `gw repos` does not count them. `gw clean` also refuses to remove any
path that is outside the managed worktrees directory.
