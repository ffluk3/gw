# The layout and discovery model

`gw` has no configuration that says where a repository is. It observes.

## Discovery is structural

A directory is a layout root if it contains `.bare/HEAD`. That single fact is
the whole convention. There is no marker file and no registry.

When you run a command, `gw` resolves the layout once, before any command
logic runs, and passes it down. Every command then works from an absolute root.

## The four starting points

`gw` accepts, in order:

1. a linked worktree, including any subdirectory of one
2. the bare repository, `<root>/.bare/`
3. the layout root, `<root>/`
4. a subdirectory of the layout root that git resolves

The second and third cases need explicit handling because the layout root is
not itself a git repository, and git reports the common directory as `.bare`.
`gw` derives the root from the shape of the path git reports: if the common
directory is named `.bare`, its parent is the root.

Anything else is an error. The error is not generic: it names the layout shape
and the two ways forward, `--repo` and `gw init`.

## Why a normal clone is rejected

In a normal clone, git reports a common directory named `.git`. Treating its
parent as a root would produce `worktrees/` under an ordinary project and
`gw` would then manage two different layouts for the same repository. `gw`
rejects the case instead.

## Why the worktree directory is inferred, not assumed

The convention says `worktrees/`, but an older checkout may use
`.bare.worktrees/`. If `gw` assumed the convention, it could create a second
worktree directory beside the real one, and the two would drift.

So `gw` reads `git worktree list` and adopts the common parent of the linked
worktrees when it differs. The rule is deliberately narrow: the directory must
be a direct child of the root and must contain at least one linked worktree. A
stray path can never widen the managed area.

## Names are resolved, paths are trusted

`--repo` distinguishes two kinds of value:

- a path that exists is used as given
- a value shaped like a path (`./x`, `/x`, `~/x`) that does not exist is an
  error, never a name

Only values that do not look like paths are resolved as names under the repos
root. This stops a typo from silently addressing a different repository.

A bare name must match exactly one layout. Ambiguity is an error that lists
the matches.
