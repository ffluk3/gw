# Repositories and discovery

`gw` has no configuration that gives the location of a repository. It
observes the directory structure.

## Discovery is structural

A directory is a layout root if it contains `.bare/HEAD`. That single fact is
the whole convention. There is no marker file and no registry.

When you run a command, `gw` resolves the layout one time, before any command
logic runs, and passes the layout down. Each command then works from an
absolute root.

## The four starting points

`gw` accepts these starting points, in this order:

1. a linked worktree, including any subdirectory of one
2. the bare repository, `<root>/.bare/`
3. the layout root, `<root>/`
4. a subdirectory of the layout root that git resolves

The second and third cases need explicit handling. The layout root is not
itself a git repository, and git reports the common directory as `.bare`. `gw`
derives the root from the shape of the path that git reports. If the common
directory is named `.bare`, its parent is the root.

Each other case is an error. The error is not generic. It gives the shape of
the layout and the two ways forward, `--repo` and `gw init`.

## Why `gw` rejects a normal clone

In a normal clone, git reports a common directory named `.git`. If `gw` treats
its parent as a root, it makes `worktrees/` under an ordinary project, and
`gw` then manages two different layouts for the same repository. `gw` rejects
this case instead.

## Why `gw` finds the worktree directory

The convention says `worktrees/`, but an older checkout can use
`.bare.worktrees/`. If `gw` assumes the convention, it can make a second
worktree directory beside the real one, and the two directories then become
different.

So `gw` reads `git worktree list` and uses the common parent of the linked
worktrees when that parent is different. The rule is deliberately narrow. The
directory must be a direct child of the root, and it must contain at least one
linked worktree. A stray path can never expand the managed area.

## Names are resolved, paths are trusted

`--repo` distinguishes two kinds of value:

- a path that exists is used as given
- a value with the shape of a path (`./x`, `/x`, `~/x`) that does not exist is
  an error, never a name

Only values that do not look like paths are resolved as names under the repos
root. This rule prevents a typo from addressing a different repository.

A bare name must match exactly one layout. Ambiguity is an error that lists the
matches.
