# Layout on disk

## The convention

```text
<root>/
  .bare/               the bare repository
  worktrees/<branch>/  one checkout per branch
```

A slash in a branch name becomes a directory level. `feature/x` lives at
`worktrees/feature/x`.

## The alternate worktrees directory

An existing checkout can use `.bare.worktrees/` instead of `worktrees/`. `gw`
uses the directory that already exists. It does not make a second directory
beside the real one.

When `gw` loads a layout, it also reads `git worktree list` and uses the common
parent of the linked worktrees if that parent is different from the convention.
`gw` accepts only a directory that is a direct child of the layout root and
that contains at least one linked worktree.

## The repos root

```text
<repos>/
  <owner>/
    <name>/          a layout root
      .bare/
      worktrees/
```

`gw` discovers both nested (`owner/name`) and flat (`name`) layouts. The repos
root is `GW_REPOS_ROOT`, then `[discovery] repos_root` in `~/.gw.toml`, then
`~/repos`.

The global file is the only file that can set it. The repos root is how `gw`
finds a layout, so it must be known before a layout's own `.gw.toml` can be
read. A `[discovery] repos_root` in a layout file is a usage error.

## How `gw` recognizes a layout

A directory is a layout root when `<dir>/.bare/HEAD` is a file. Nothing else is
necessary, and `gw` does not write a marker file.

## Discovery from the current directory

`gw` accepts these starting points:

1. a linked worktree, including a subdirectory of one
2. the bare repository itself, `<root>/.bare/`
3. the layout root, `<root>/`
4. a subdirectory of the layout root that git can resolve

Each other location is an error. A normal clone, with a `.git` directory in the
project, is also an error, because git's common directory is not `.bare`.

## Agent harness worktrees

Paths that contain `/.claude/worktrees/` belong to the harness. `gw clean`
skips them and `gw repos` does not count them. `gw clean` also refuses to
remove a path that is outside the managed worktrees directory.
