# gw

Manage git worktrees in a bare-repo layout.

You're reviewing PROJ-101 when a bug lands in PROJ-102. Normally you stash,
switch, fix, switch back, and hope the stash still applies.

`gw` gives every branch its own checkout. You change tasks by changing
directory — nothing is stashed, and nothing is lost.

```text
# Without gw                          # With gw
git stash                             gw add --cd PROJ-102
git switch PROJ-102                   ... fix the bug ...
... fix the bug ...                   cd ../PROJ-101
git switch PROJ-101
git stash pop    # hope it applies
```

`gw` manages a **layout**: one bare repository (history, no files) plus one
checkout per branch.

```text
<root>/
  .bare/               the shared repository
  worktrees/<branch>/  one checkout per branch
```

## Requirements

- `git` on your `PATH`.
- A Rust toolchain at 1.87.0 or newer, **only if you build from source**.

You need these tools only for optional features:

- `gh`, for the merged-pull-request check in `gw clean`
- `curl`, for JIRA lookups

## Install

Install the latest release. This method needs `curl` and `tar`, not a Rust
toolchain:

```bash
curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh | bash
```

Or install from source, which builds with the pinned toolchain:

```bash
git clone https://github.com/ffluk3/gw.git
cd gw
./install.sh --from-source
```

Each method writes `gw` to `~/.local/bin`. Then add the shell integration,
which makes `gw add --cd` change into the new worktree:

```bash
echo 'source ~/.local/share/gw/gw.sh' >> ~/.zshrc
```

Start a new shell and make sure that the install is correct:

```bash
gw --version
```

To pin a version, pass `--version v1.2.3`. To install to another directory,
pass `--dir DIR`.

## Why not just use `git worktree`?

Git already has worktrees; `gw` is a thin layer over them and does not replace
`git`. What it adds is the bookkeeping:

| You want to... | Plain git | `gw` |
| --- | --- | --- |
| Open a ticket you already opened | `git worktree add` fails: the branch is checked out | `gw add` reuses the checkout |
| Delete a pushed, unmerged branch | `git branch -d` refuses; you use `-D` and hope | Deletes after proving every commit is on a remote |
| Know what to clean up | `git worktree list`, then judge each one | `gw clean` prints a plan with a reason per row |

[The docs](docs/index.md) have the full comparison, including what `gw`
deliberately does not do.

## Quick start

```bash
# make a layout
gw init git@github.com:acme/widgets.git --dir ~/repos/acme/widgets

# open a worktree and enter it
cd ~/repos/acme/widgets/worktrees/main
gw add --cd PROJ-1234

# see each worktree
gw status

# preview cleanup, then apply it
gw clean --dry-run
gw clean --yes
```

## Documentation

The full documentation is the MkDocs site in [`docs/`](docs/index.md). Start
with the [tutorial](docs/tutorials/getting-started.md), or go straight to the
[User Manual](docs/explanation/index.md) to understand the model, the
[how-to guides](docs/how-to/index.md) for a task, or the
[reference](docs/reference/index.md) for a flag. To use `gw` from an AI agent,
see the [skill](skills/gw/SKILL.md).

To preview the site locally:

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements-docs.txt
mkdocs serve
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md#documentation) for the full docs setup
and the strict build that CI runs.

## Development

```bash
./scripts/install-hooks.sh    # once per clone: validate commit messages
cargo test                    # unit, property, and real-git integration tests
cargo clippy --all-targets
cargo fmt --all -- --check
```

Commits follow [Conventional Commits](https://www.conventionalcommits.org/).
A `commit-msg` hook rejects messages that do not, and CI checks each commit in
a pull request. The message format sets the next release version, and
release-plz computes that version from the history. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for the rules and the release flow.

The integration tests use real git repositories in a temporary directory. They
do not use mocks, because the behavior that matters is the behavior of git.

## License

MIT. See `LICENSE`.
