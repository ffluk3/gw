# Architecture

The workspace has two crates. The split exists so that you can test the policy
that decides what gets destroyed without a repository.

## `crates/gw-core` — the policy

Pure domain. There is no filesystem, no process, no network, and no
environment. Each function is total over facts that the adapter layer already
collected.

| Module | Responsibility |
| --- | --- |
| `model` | Typed values: `BranchName`, `Sha`, `TicketKey`, and their validation |
| `snapshot` | The facts that a decision can see: `RepoSnapshot`, `Dirty`, `Activity`, `ReachableSet` |
| `classify` | The verdict functions and the clean plan |
| `decision` | `Target`, `Decision`, `Reason`, `Plan`, `Counts` |
| `policy` | The thresholds and opt-ins |
| `convention` | What a branch name means: ordered rules that map a branch to provider queries |
| `layout` | The on-disk convention |
| `render` | Table, human text, and JSON rendering |

Because the environment is absent, nothing in this crate reads a flag or a
variable. `gw` constructs the policy at the CLI boundary and passes it by
reference.

## `crates/gw` — the shell

The adapter layer. It parses arguments, discovers the layout, runs git, GitHub,
and JIRA, and applies plans.

| Module | Responsibility |
| --- | --- |
| `cli` | Argument parsing |
| `config` | Layout discovery and root normalization |
| `context` | Wires the layout, git handle, and configuration one time, and computes the default-branch layer from `origin/HEAD` |
| `convention_file` | Parsing the global, default-branch, and layout `.gw.toml` files, and merging them |
| `settings` | The run settings derived from the merged files |
| `git` | Typed git operations behind `GitBackend` |
| `providers` | Capability traits, the registry, and one adapter per system |
| `repos` | Repos-root discovery and name resolution |
| `commands` | One runner for each subcommand |

## Why a typed git trait

`GitBackend` is a trait with one method for each operation, not a generic
`run(args)`. This has two results: a fake is easy to write, and each failure
mode has a name in the type system instead of a string in a log.

## Why `curl` instead of an HTTP client

The only network calls are one batched JQL post and the GitHub CLI. `curl`
does not link a TLS stack into a tool that does not need one, and it keeps the
build dependency-light. It also matches what the previous shell implementation
needed.

Credentials go to `curl` through a mode-0600 configuration file rather than
`-u`, so they never appear in the process list.

## Why the tests use real git

The integration tests make real repositories in a temporary directory. The
behavior that matters is the behavior of git. That includes what
`worktree list` reports, what `rev-list --remotes` contains, and how a
fast-forward fails when another worktree checks out the branch. A mock encodes
an assumption about those answers, and those answers are exactly what the
tests must check.

## Command flow

```text
main
  -> parse arguments
  -> init / repos: run directly, no layout needed
  -> otherwise: resolve and load Context
       -> discover layout
       -> infer worktrees directory
       -> load settings (global, default-branch, layout)
  -> command runner
       -> gather snapshot (I/O)
       -> classify (pure)
       -> render or execute
```

See [the decision model](decision-model.md) for the clean-specific path.
