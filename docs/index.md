# gw

A normal git clone gives you one checkout and one branch at a time. To work on
a second branch, you switch. Switching means that you commit your work or
stash it. `gw` manages a **bare-repo worktree layout** instead. You get one
checkout for each branch, and one repository backs them all. You change
branches by changing directory.

```text
<root>/
  .bare/               the shared repository
  worktrees/<branch>/  one checkout for each branch
```

There is no primary checkout. Each checkout is a linked worktree of the bare
repository, so every worktree behaves the same way.

## Install

Install the latest release. You need `curl` and `tar`, not a Rust toolchain:

```bash
curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh | bash
```

Then load the shell integration. It gives you `gwadd` and `gw add --cd`:

```bash
echo 'source ~/.local/share/gw/gw.sh' >> ~/.zshrc
```

## The core loop

```bash
# make a layout: a bare repo and a checkout of the default branch
gw init git@github.com:acme/widgets.git --dir ~/repos/acme/widgets

# enter the first checkout; gw finds the layout by walking up
cd ~/repos/acme/widgets/worktrees/main

# open a worktree for a ticket and change into it
gwadd PROJ-1234

# see every worktree in one table
gw status

# preview cleanup, then apply it
gw clean --dry-run
gw clean --yes
```

`gwadd` creates the branch from the current `HEAD` when the branch does not
exist. Run the command again for the same branch, and `gw` reuses the existing
worktree. It does not make a second one.

## What `gw` adds over plain git

Git already supports worktrees. `gw` adds the parts that are tedious by hand:

- one command to open or reuse a worktree, for local, remote, and new branches
- one status view for each worktree in a layout
- a cleanup pass that proves publication before it removes anything
- a way to address a layout from any directory, by root or by name

`gw` has no built-in host, repository, or ticket system. Put per-layout
configuration in `.gw.toml`. Put credentials in the environment only.

## Deletion is safe by default

`gw clean` is the only command that destroys anything, so its rules **fail
closed**. `gw` removes a worktree only when every protecting fact is absent.
The tree must be clean. Each commit must be on a remote. The pull request must
be merged, or the branch must be stale.

A fact that `gw` cannot observe counts as unsafe. Unreadable cleanliness counts
as dirty. Activity from an unknown source is never stale.

Run `gw clean` without `--yes` to see the plan. [The safety
rules](explanation/clean-safety.md) give the full ordered list of checks.

## Where to go next

This documentation follows the [Diátaxis](https://diataxis.fr/) framework. Each
page does one job:

| If you want to... | Read |
| --- | --- |
| learn by doing, from zero | [Tutorial: get started](tutorials/getting-started.md) |
| finish a specific task | [How-to guides](how-to/index.md) |
| look up a flag, variable, or output shape | [Reference](reference/index.md) |
| understand why `gw` works this way | [Explanation](explanation/index.md) |

The [tutorial](tutorials/getting-started.md) takes about ten minutes and leaves
you with a working layout. Common starting points:

- [Command reference](reference/cli.md)
- [Configuration reference](reference/configuration.md)
- [Use `gw` in an agent workflow](how-to/integrate-with-agents.md)
