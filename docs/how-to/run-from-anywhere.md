# Run `gw` from outside a layout

Use this guide when your shell is not inside a `gw` layout.

## Address an existing layout

```bash
gw --repo ~/repos/acme/widgets status
gw --repo ~/repos/acme/widgets/worktrees/main status
```

`--repo` also accepts `<root>/.bare`. `gw` uses the parent directory.

## Address a layout by name

```bash
gw --repo acme/widgets status
gw --repo widgets status
```

Names resolve under `GW_REPOS_ROOT`, which defaults to `~/repos`.

## Use the environment variable

```bash
export GW_REPO=acme/widgets
gw status
gw add PROJ-500
```

`GW_REPO` and `--repo` accept the same values.

## Name a path that does not exist

A value that has the shape of a path but does not exist is an error. `gw` does
not treat the value as a repository name. This rule prevents a typo from
addressing the wrong repository:

```bash
gw --repo ./missing status    # error: ./missing is not a directory
```

## What `gw` does without `--repo`

`gw` walks up from the current directory and stops at the first directory that
has a `.bare/HEAD` file. If it finds no such directory, or the directory is a
normal clone, it stops and prints the ways forward.

If you are not sure which layout `gw` will use, pass `--repo` explicitly, or
run `gw repos`. `gw repos` prints the root that it scans.
