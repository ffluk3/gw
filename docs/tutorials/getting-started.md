# Get started with `gw`

This tutorial shows you how to install `gw`, make your first layout, and open
a worktree for a branch. You do not need to know how worktrees work. You need
`git`, a terminal, and a repository you can clone.

## Before you start

You need:

- `git` on your `PATH`.
- A remote to clone. The examples use `git@github.com:acme/widgets.git`.

To install the prebuilt release you also need `curl` and `tar`. To build from
source you need a Rust toolchain at 1.87.0 or newer. `rust-toolchain.toml` pins
the exact version, so `rustup` selects it for you.

Optional tools are only needed for optional features:

- `gh`, for the merged-pull-request check in `gw clean`.
- `curl`, for JIRA lookups.

## Install

You can install a prebuilt release or build from source. Both write `gw` to
`~/.local/bin`. If that directory is not on your `PATH`, the script prints a
warning.

=== "Prebuilt release"

    Install the latest release. This needs no Rust toolchain:

    ```bash
    curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh | bash
    ```

    The installer downloads the archive for your platform, checks its SHA-256
    sum, and installs the binary. To pin a version, pass `--version v1.2.3`:

    ```bash
    curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh \
      | bash -s -- --version v1.2.3
    ```

=== "From source"

    Clone the repository and build it with the pinned toolchain:

    ```bash
    git clone https://github.com/ffluk3/gw.git
    cd gw
    ./install.sh --from-source
    ```

Then add the shell integration. This step gives you `gwadd` and `gw add --cd`,
which open a worktree and change into it:

```bash
echo 'source ~/.local/share/gw/gw.sh' >> ~/.zshrc
```

Start a new shell and confirm the install:

```bash
gw --version
```

## Make your first layout

A layout is a directory with a bare repository and one checkout for each
branch. `gw init` creates one.

```bash
gw init git@github.com:acme/widgets.git --dir ~/repos/acme/widgets
```

`gw` prints the steps it takes: it clones the bare repository to
`<root>/.bare`, then checks out the default branch at
`<root>/worktrees/<branch>/`.

To see the plan without writing anything, add `--dry-run`.

## Confirm that `gw` finds the layout

Change into the first checkout:

```bash
cd ~/repos/acme/widgets/worktrees/main
```

Now ask for the status:

```bash
gw status
```

You did not tell `gw` where the layout is. It walked up from the current
directory and found it. The table lists one row per worktree.

## Open a worktree for a branch

```bash
gwadd PROJ-1234
```

`gwadd` makes the worktree and changes your directory into it. If the branch
does not exist yet, `gw` makes it from the current `HEAD`.

Try the same command again:

```bash
gwadd PROJ-1234
```

This time `gw` reuses the existing worktree. It does not make a second one.

## Check what you made

```bash
gw status
```

You see a row for each worktree, with its last commit age, a dirty marker, and
the JIRA summary when that is configured.

## Remove a worktree safely

`gw clean` removes worktrees. It is the one destructive command, so look at the
plan first:

```bash
gw clean --dry-run
```

Read the plan. When you agree with it, apply it:

```bash
gw clean --yes
```

`gw` removes a worktree only when it can prove that the work is safe: every
commit is on a remote, the tree is clean, and the PR is merged or the branch is
stale. [The safety rules](../explanation/clean-safety.md) explain each check.

## Next steps

- Learn the everyday commands in the [how-to guides](../how-to/index.md).
- Look up flags in the [command reference](../reference/cli.md).
- Configure JIRA summaries in the
  [configuration reference](../reference/configuration.md).
