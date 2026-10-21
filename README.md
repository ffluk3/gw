# gw

Manage git worktrees in a bare-repo layout.

`gw` keeps one checkout for each branch, all backed by one bare repository.
To change a branch, you change directory. You do not stash your work.

```text
<root>/
  .bare/               the bare repository
  worktrees/<branch>/  one checkout per branch
```

## Requirements

- Rust 1.87.0 or newer. `rust-toolchain.toml` pins the exact version, so
  `rustup` selects it for you.
- `git` on your `PATH`.

Optional, for optional features:

- `gh`, for the merged-pull-request check in `gw clean`
- `curl`, for JIRA lookups

## Install

```bash
git clone https://github.com/ffluk3/gw.git
cd gw
./install.sh
```

This builds `gw` and installs it to `~/.local/bin`. Then add the shell
integration, which provides `gwadd`:

```bash
echo 'source /path/to/gw/shell/gw.sh' >> ~/.zshrc
```

Start a new shell and check the install:

```bash
gw --version
```

## Quick start

```bash
# create a layout
gw init git@github.com:acme/widgets.git --dir ~/repos/acme/widgets

# open a worktree and enter it
cd ~/repos/acme/widgets/worktrees/main
gwadd PROJ-1234

# see every worktree
gw status

# preview cleanup, then apply it
gw clean --dry-run
gw clean --yes
```

## Documentation

Full documentation lives in [`docs/`](docs/index.md) and is organized by the
[Diátaxis](https://diataxis.fr/) framework:

| Goal | Start here |
| --- | --- |
| Learn the tool from zero | [Tutorial](docs/tutorials/getting-started.md) |
| Do a specific task | [How-to guides](docs/how-to/index.md) |
| Look up a flag or setting | [Reference](docs/reference/index.md) |
| Understand the design | [Explanation](docs/explanation/index.md) |

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
A `commit-msg` hook rejects messages that do not, and CI checks every commit in
a pull request. The format decides the next release version, which
release-plz computes from the history. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for the rules and the release flow.

The integration tests use real git repositories in a temporary directory. They
do not use mocks, because the behavior that matters is the behavior of git.

## License

MIT. See `LICENSE`.
