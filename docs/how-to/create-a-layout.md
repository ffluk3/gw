# Create a layout

Use this guide to turn a remote into a bare-repo layout.

## Create with a chosen directory

```bash
gw init git@github.com:acme/widgets.git --dir ~/src/widgets
```

## Let `gw` choose the directory

```bash
gw init git@github.com:acme/widgets.git
```

The default is `<repos>/<owner>/<name>`, where `<repos>` is `~/repos` unless
you set `GW_REPOS_ROOT`. The example above becomes `~/repos/acme/widgets`.

A local remote gets no owner component:

```bash
gw init /srv/git/widgets.git      # ~/repos/widgets
```

## Choose the first branch

```bash
gw init git@github.com:acme/widgets.git --dir ~/src/widgets --branch develop
```

Without `--branch`, `gw` checks out the remote's default branch.

## Preview before writing

```bash
gw init git@github.com:acme/widgets.git --dir ~/src/widgets --dry-run
```

## If the directory is already a layout

`gw init` refuses to write into an existing layout. It stops and tells you to
use the existing one:

```bash
gw --repo ~/src/widgets status
```

See [the layout explanation](../explanation/layout.md) for why this is a hard
stop.
