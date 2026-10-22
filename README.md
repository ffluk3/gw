# gw

Manage git worktrees in a bare-repo layout.

`gw` keeps one checkout for each branch, and one bare repository backs them
all. To change a branch, you change directory. You do not stash your work.

```text
<root>/
  .bare/               the bare repository
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
which provides `gwadd` and makes `gw add --cd` change into the new worktree:

```bash
echo 'source ~/.local/share/gw/gw.sh' >> ~/.zshrc
```

Start a new shell and make sure that the install is correct:

```bash
gw --version
```

To pin a version, pass `--version v1.2.3`. To install to another directory,
pass `--dir DIR`.

## Quick start

```bash
# make a layout
gw init git@github.com:acme/widgets.git --dir ~/repos/acme/widgets

# open a worktree and enter it
cd ~/repos/acme/widgets/worktrees/main
gwadd PROJ-1234

# see each worktree
gw status

# preview cleanup, then apply it
gw clean --dry-run
gw clean --yes
```

## Documentation

The full documentation is in [`docs/`](docs/index.md). It follows the
[Diátaxis](https://diataxis.fr/) framework:

| Goal | Start here |
| --- | --- |
| Learn the tool from zero | [Tutorial](docs/tutorials/getting-started.md) |
| Do a specific task | [How-to guides](docs/how-to/index.md) |
| Look up a flag or setting | [Reference](docs/reference/index.md) |
| Understand the design | [Explanation](docs/explanation/index.md) |
| Use `gw` from an AI agent | [Agent skill](skills/gw/SKILL.md) |

The docs are a MkDocs site. To preview them locally:

```bash
pip install mkdocs-material
mkdocs serve
```

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
