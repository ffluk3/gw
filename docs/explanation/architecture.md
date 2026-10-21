# Architecture

The workspace has two crates. The split exists so the policy that decides what
gets destroyed can be tested without a repository.

## `crates/gw-core` — the policy

Pure domain. No filesystem, no processes, no network, no environment. Every
function is total over already-collected facts.

| Module | Responsibility |
| --- | --- |
| `model` | Typed values: `BranchName`, `Sha`, `TicketKey`, and their validation |
| `snapshot` | The facts a decision may see: `RepoSnapshot`, `Dirty`, `Activity`, `ReachableSet` |
| `classify` | The verdict functions and the clean plan |
| `decision` | `Target`, `Decision`, `Reason`, `Plan`, `Counts` |
| `policy` | The thresholds and opt-ins |
| `layout` | The on-disk convention |
| `render` | Table, human text, and JSON rendering |

Because the environment is absent, nothing here reads a flag or a variable.
Policy is constructed at the CLI boundary and passed by reference.

## `crates/gw` — the shell

The adapter layer. It parses arguments, discovers the layout, runs git, GitHub,
and JIRA, and applies plans.

| Module | Responsibility |
| --- | --- |
| `cli` | Argument parsing |
| `config` | Layout discovery and root normalization |
| `context` | Wires the layout, git handle, and settings once |
| `settings` | `.gw.toml` and precedence with flags |
| `git` | Typed git operations behind `GitBackend` |
| `github` | Batched `gh api graphql` merged-PR lookup |
| `jira` | Batched JQL lookup over `curl` |
| `repos` | Repos-root discovery and name resolution |
| `commands` | One runner per subcommand |

## Why a typed git trait

`GitBackend` is a trait with one method per operation, not a generic
`run(args)`. Two things follow: a fake is trivial to write, and every failure
mode has a name in the type system instead of being a string in a log.

## Why `curl` instead of an HTTP client

The only network calls are one batched JQL post and the GitHub CLI. Using
`curl` avoids linking a TLS stack into a tool that otherwise does not need one,
and keeps the build dependency-light. It also matches what the previous shell
implementation required.

Credentials go to `curl` through a mode-0600 config file rather than `-u`, so
they never appear in the process list.

## Why the tests use real git

The integration tests make real repositories in a temporary directory. The
behavior that matters is git's behavior: what `worktree list` reports, what
`rev-list --remotes` includes, how a fast-forward fails when a branch is
checked out elsewhere. Mocks would encode assumptions about those answers,
which is exactly what the tests need to check.

## Command flow

```text
main
  -> parse arguments
  -> init / repos: run directly, no layout needed
  -> otherwise: resolve and load Context
       -> discover layout
       -> infer worktrees directory
       -> load settings
  -> command runner
       -> gather snapshot (I/O)
       -> classify (pure)
       -> render or execute
```

See [the decision model](decision-model.md) for the clean-specific path.
