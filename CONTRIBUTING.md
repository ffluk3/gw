# Contributing to gw

## Set up hooks after cloning

The repository checks every commit message. Enable the check once:

```bash
./scripts/install-hooks.sh
```

This sets `core.hooksPath` to `.githooks/`, so git uses the checked-in
`commit-msg` hook. The hook and CI run the same script, so a message that
passes locally passes in CI.

## Commit messages

Every commit follows [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>)!: <description>
```

- **type** is one of `feat`, `fix`, `docs`, `style`, `refactor`, `perf`,
  `test`, `build`, `ci`, `chore`, `revert`.
- **scope** is optional and lower-case, for example `(clean)` or `(git)`.
- **!** marks a breaking change.
- **description** is a short summary. No leading space after the colon.

The header is limited to 100 characters.

```text
feat(clean): skip worktrees with uncommitted changes
fix: resolve the repos root from the layout
refactor!: drop the legacy shell implementation
```

### How commits pick the next version

The version in `Cargo.toml` is set by the commit history, not by hand.

| Commit | Version bump |
| --- | --- |
| `feat:` | minor |
| `fix:` | patch |
| any type with `!` | major |
| any type with a `BREAKING CHANGE:` footer | major |
| `docs:`, `test:`, `ci:`, `chore:`, `build:`, `style:`, `perf:`, `refactor:` | none on their own |

A commit with no bump still lands. It joins the next release when another
commit triggers one.

## What happens on `main`

1. A push to `main` runs the `release-plz` workflow.
2. `release-plz` opens a **release pull request** that bumps the workspace
   version, updates `CHANGELOG.md`, and adds the commits since the last tag.
3. Merging that pull request tags the commit (`vX.Y.Z`) and creates a GitHub
   release.
4. The `upload-binaries` job in the same workflow cross-compiles `gw` and
   attaches one archive and one `.sha256` file for each supported platform:

   | Target | Runs on |
   | --- | --- |
   | `x86_64-unknown-linux-musl` | ubuntu |
   | `aarch64-unknown-linux-musl` | ubuntu (via `cross`) |
   | `x86_64-apple-darwin` | macOS |
   | `aarch64-apple-darwin` | macOS |

   `install.sh` downloads the archive for the running platform and verifies
   the checksum before it installs the binary.

`gw` is distributed through GitHub Releases, so nothing is published to
crates.io. `release-plz.toml` sets `git_only = true` for that reason.

## Before you open a pull request

```bash
./scripts/check-commit-message.sh --range origin/main..HEAD
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```
