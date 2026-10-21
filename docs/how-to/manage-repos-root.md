# Manage many repositories under one root

Use this guide when you keep several project layouts in one parent directory.

## Use the default root

Layouts live under `~/repos`, organized as `<owner>/<name>`:

```text
~/repos/
  acme/
    widgets/
      .bare/
      worktrees/
    other/
      .bare/
      worktrees/
```

## Use a different root

```bash
export GW_REPOS_ROOT=~/src/work
```

A leading `~/` is expanded by `gw`, even when the variable comes from a program
rather than a shell.

## List all layouts

```bash
gw repos
```

Add `--dirty` to count worktrees with uncommitted changes. This runs one
`git status` per worktree, so it is opt-in.

```bash
gw repos --dirty
```

## Select a repository by name

```bash
gw --repo acme/widgets status   # owner/name
gw --repo widgets status        # bare name, if it is unique
```

A bare name that matches more than one layout is an error, and the error lists
the matches. Add the owner to disambiguate.

## Select a repository with an environment variable

`GW_REPO` is the environment form of `--repo`:

```bash
GW_REPO=acme/widgets gw status
```

This is useful in a script that is not inside any layout.
