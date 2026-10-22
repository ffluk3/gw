# Create a layout

Use this guide to turn a remote into a bare-repo layout.

## Create a layout in a directory that you select

```bash
gw init git@github.com:acme/widgets.git --dir ~/src/widgets
```

## Let `gw` select the directory

```bash
gw init git@github.com:acme/widgets.git
```

The default is `<repos>/<owner>/<name>`. `<repos>` is `~/repos` unless you set
`GW_REPOS_ROOT`. The example above becomes `~/repos/acme/widgets`.

A local remote has no owner component:

```bash
gw init /srv/git/widgets.git      # ~/repos/widgets
```

## Select the first branch

```bash
gw init git@github.com:acme/widgets.git --dir ~/src/widgets --branch develop
```

Without `--branch`, `gw` checks out the default branch of the remote.

## Preview before you write

```bash
gw init git@github.com:acme/widgets.git --dir ~/src/widgets --dry-run
```

## If the directory is already a layout

`gw init` does not write into an existing layout. It stops and gives you the
command for the existing layout:

```bash
gw --repo ~/src/widgets status
```

See [the layout explanation](../explanation/layout.md) for why this is a hard
stop.
