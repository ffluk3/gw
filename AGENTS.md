# AGENTS.md

Guidance for an agent working in this repository. Read this before changing code.

## What this repo is

`gw` is a Rust CLI that manages git worktrees in a **bare-repo layout**. One
checkout per branch, one bare repo behind them all:

```text
<root>/
  .bare/               the bare repository (contains HEAD)
  worktrees/<branch>/  one checkout per branch
  .gw.toml             optional per-layout settings
```

`gw` does not store configuration pointing at a repository. It discovers the
layout from the current directory, by walking up until it finds a `.bare/HEAD`.
Do not add a registry, marker file, or global config for layout location.

It is distributed through GitHub Releases (`ffluk3/gw`), never crates.io. Users
install with `install.sh` (release download or `--from-source`).

## Layout of the code

Two crates. The split is the most important structural rule in the repo.

### `crates/gw-core` — pure domain, no I/O

- **No filesystem, processes, network, or environment.** Everything is a total
  function over facts already collected by the adapter layer.
- Why: the policy that decides what gets destroyed must be testable in
  isolation from git.
- Modules: `model` (validated `BranchName`, `Sha`, `TicketKey`), `snapshot`
  (facts a decision may see), `classify` (verdicts and clean plan), `decision`
  (`Target`, `Decision`, `Reason`, `Plan`), `policy` (thresholds/opt-ins),
  `layout`, `render`.

If you find yourself wanting `std::env` or `std::fs` in `gw-core`, the logic is
in the wrong crate. Put the fact in `snapshot` and the rule in `classify`.

### `crates/gw` — adapter shell

Parses args, discovers the layout, performs I/O, and applies plans.

- `cli` (clap), `config` (layout discovery), `context` (wires layout + git +
  settings), `settings` (`.gw.toml`), `git` (`GitBackend` trait), `github`
  (`gh api graphql`), `jira` (batched JQL over `curl`), `repos`, `commands`.
- `GitBackend` is a **typed trait**, one method per operation, not
  `run(args)`. Keep it that way: failures stay named and a fake stays trivial.
- Network access is intentionally `curl` + `gh`, not an HTTP client library.
  Do not add a TLS stack. Credentials go through a mode-0600 curl config file,
  never `-u` on the command line.

## Non-negotiable invariants

These are the behavior contracts; breaking one is a bug even if tests pass.

1. **`gw clean` fails closed.** Unobserved state is never a reason to remove.
   `Cleanliness` is three-state (`clean`/`dirty`/`unknown`); `unknown` counts as
   dirty. Activity with an unknown source is never stale.
2. **Order of verdicts is policy.** Every protecting fact is tested before any
   destroying fact. `Verdict` is exhaustive and `decide` has no default arm, so
   a new fact must get its consequence written down to compile. Preserve that
   property. See `docs/explanation/clean-safety.md` for the ordered list.
3. **The executor never re-derives a decision.** Dry-run output is exactly what
   runs. A `Delete` carries its tip SHA in the type; it cannot be constructed
   without restore data.
4. **Publishing is reachability from any remote**, not `origin/<branch>`.
5. **Branch deletion and worktree removal protect different sets.** Deletion
   also protects branches checked out in a worktree; removal does not. Do not
   conflate them.
6. **A normal clone (`.git`) is rejected**, not silently treated as a layout.
7. **The JSON shape is the stable contract** (`docs/reference/json.md`). Tables,
   widths, emoji, and color are not. Scripts target JSON.
8. **Exit codes are contractual:** 0 success (including a no-op dry run), 1
   operation failed, 2 usage/config error, 3 partial destructive failure.

## Build and verify

Pinned toolchain: **1.87.0** (`rust-toolchain.toml`). Do not bump it casually;
a newer clippy can turn a previously passing lint into `-D warnings` failure.

Use `cargo` directly (this repo has no Gradle; ignore any generic global rules
about gradle in this project).

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo llvm-cov --all --fail-under-lines 95
```

The full PR gate adds:

```bash
./scripts/check-commit-message.sh --range origin/main..HEAD
```

CI (`ci.yml`) runs fmt, clippy, tests on Linux and macOS, the coverage gate,
and `shellcheck` on `install.sh` and `shell/gw.sh` on Linux. Keep shell changes
shellcheck-clean.

### Coverage

Line coverage is a gate, not a target: **95% minimum**, enforced in CI and
locally by the command above. `cargo-llvm-cov` is the only extra tool:

```bash
cargo install cargo-llvm-cov --locked
```

Two details make the number honest.

- **The end-to-end tests count.** `crates/gw/tests/cli.rs` runs the real `gw`
  binary, and `cargo-llvm-cov` merges the profile that binary writes. The
  command layer is therefore measured even though the test process never calls
  it directly. Do not replace those tests with in-process ones to raise a
  number; they cover the code because they exercise the real path.
- **External commands are faked at the process boundary, not mocked in code.**
  The GitHub adapter runs `gh` and the JIRA adapter runs `curl`. The tests put
  a stand-in early on `PATH` and let the real adapter build the arguments and
  parse the response. A missing, failing, or malformed response must stay
  `Unknown`: those tests are asserting the fail-closed rule, not just coverage.

A test that mutates process environment (`PATH`, `TMPDIR`, `HOME`,
`GW_REPOS_ROOT`) must hold a lock, because `cargo test` runs tests in parallel.
Prefer an absolute program path (`/bin/cat`) over one resolved through `PATH`
where the test does not need the override.

## Tests

- `crates/gw/tests/cli.rs` — end-to-end against **real git** in temp dirs. No
  mocks: git's own worktree semantics are the behavior under test. Origin must
  be a distinct repo; pointing it at the bare repo would mirror every branch
  into `refs/remotes/*` and make nothing look unpublished.
- `crates/gw-core/tests/props.rs` — generated-state safety invariants, phrased
  positively ("never destroys X unless Y held").
- Unit tests live beside the module.
- Adapter tests put a stand-in `gh` or `curl` early on `PATH` and run the real
  binary, so argument building and response parsing are covered without a mock.

When adding a rule, add the property that says what must never happen, not just
an example that says what should. See `docs/developer/testing.md` for the
coverage gate and the environment-locking rule.

## Commit and release rules

- **Conventional Commits are enforced** by `.githooks/commit-msg` and CI.
  Format `<type>(<scope>)!: <description>`, header ≤ 100 chars, no space after
  the colon.
- Valid types: `feat fix docs style refactor perf test build ci chore revert`.
- The message **sets the next version** via release-plz: `feat`→minor,
  `fix`→patch, `!`/`BREAKING CHANGE:`→major. Others bump nothing on their own.
- `release-plz` on `main` opens a release PR (version + CHANGELOG), and merging
  it tags `vX.Y.Z`, creates the GitHub release, and cross-compiles four targets
  with checksums.
- `gw` and `gw-core` share `[workspace.package].version`. Never edit versions
  by hand; never publish to a registry.

Run `./scripts/install-hooks.sh` once per clone.

## Documentation

Docs are MkDocs + Material, organized by Diátaxis, under `docs/`:

| Directory | Purpose |
| --- | --- |
| `docs/tutorials/` | Learning from zero |
| `docs/how-to/` | Task recipes |
| `docs/reference/` | Flags, config, JSON, exit codes, layout |
| `docs/explanation/` | Design rationale for users (nav: "User Manual") |
| `docs/developer/` | Design rationale for contributors (nav: "Developer Manual") |

`mkdocs.yml` sets `strict: true`, and CI builds with `mkdocs build --strict`;
a page not in `nav:` or a broken link fails the build. `site/` is generated and
git-ignored — never edit it.

**A behavior change is not done until the matching doc is updated.** The
authoritative rationale lives in `docs/developer/architecture.md`,
`decision-model.md`, `conventions.md`, `layout.md`, and
`docs/explanation/clean-safety.md`; read the relevant one before changing that
area, and keep it true. Follow `docs/developer/docs-style.md`: narrative pages
(home, User Manual, Developer Manual) use plain English, while how-to steps,
reference tables, and the ordered safety checks stay in Simplified Technical
English.

Do not rename `docs/explanation/`. Its nav label is "User Manual" but
`skills/gw/SKILL.md` links the published pages by absolute GitHub URL.

## Scope discipline

This tool is deliberately generic: nothing about a specific host, repository,
or ticket system is built in. Per-layout specifics (JIRA project keys, base
URL) belong in `.gw.toml`; credentials belong in the environment only. Do not
commit personal or company-specific values into the code or docs.
