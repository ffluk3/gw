# Get started with `gw`

This tutorial teaches you how to install `gw`, make your first layout, and
open a worktree for a branch. You do not need to know how worktrees work. You need
`git`, a terminal, and a repository that you can clone.

## Before you start

You need:

- `git` on your `PATH`.
- A remote to clone. The examples use `git@github.com:acme/widgets.git`.

To install the prebuilt release, you also need `curl` and `tar`. To build from
source, you need a Rust toolchain at 1.87.0 or newer. `rust-toolchain.toml`
pins the exact version, so `rustup` selects it for you.

You need these tools only for optional features:

- `gh`, for the merged-pull-request check in `gw clean`.
- `curl`, for JIRA lookups.

## Install

You can install a prebuilt release or build from source. Each method writes
`gw` to `~/.local/bin`. If that directory is not on your `PATH`, the script
prints a warning.

=== "Prebuilt release"

    Install the latest release. This method needs no Rust toolchain:

    ```bash
    curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh | bash
    ```

    The installer downloads the archive for your platform, makes sure that its
    SHA-256 sum is correct, and installs the binary. To pin a version, pass
    `--version v1.2.3`:

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

Start a new shell and make sure that the install is correct:

```bash
gw --version
```

## Make your first layout

A layout is a directory with a bare repository and one checkout for each
branch. `gw init` makes one.

```bash
gw init git@github.com:acme/widgets.git --dir ~/repos/acme/widgets
```

`gw` prints each step that it takes. It clones the bare repository to
`<root>/.bare`. Then it checks out the default branch at
`<root>/worktrees/<branch>/`.

To see the plan without writing anything, add `--dry-run`.

## Make sure that `gw` finds the layout

Change into the first checkout:

```bash
cd ~/repos/acme/widgets/worktrees/main
```

Now ask for the status:

```bash
gw status
```

You did not tell `gw` where the layout is. It walked up from the current
directory and found the layout. The table has one row for each worktree.

## Open a worktree for a branch

```bash
gwadd PROJ-1234
```

`gwadd` makes the worktree and changes your directory into it. If the branch
does not exist, `gw` makes it from the current `HEAD`.

Run the same command again:

```bash
gwadd PROJ-1234
```

This time, `gw` reuses the existing worktree. It does not make a second one.

## Make sure that the worktree is correct

```bash
gw status
```

You see one row for each worktree. Each row has the age of the last commit, a
dirty marker, and the JIRA summary when JIRA is configured.

## Remove a worktree safely

`gw clean` removes worktrees. It is the only destructive command, so look at
the plan first:

```bash
gw clean --dry-run
```

Read the plan. When you agree with the plan, apply it:

```bash
gw clean --yes
```

`gw` removes a worktree only when it can prove that the work is safe. Each
commit must be on a remote. The tree must be clean. The PR must be merged, or
the branch must be stale. [The safety rules](../explanation/clean-safety.md)
explain each check.

## Next steps

- Learn the everyday commands in the [how-to guides](../how-to/index.md).
- Look up flags in the [command reference](../reference/cli.md).
- Configure JIRA summaries in the
  [configuration reference](../reference/configuration.md).
