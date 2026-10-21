# gw

Manage git worktrees in a bare-repo layout.

`gw` keeps one checkout for each branch. All the checkouts use one bare
repository. To change a branch, you change directory. You do not stash your
work.

## The layout

The repository has this structure:

```
<root>/
  .bare/               the bare repository
  worktrees/<branch>/  one checkout for each branch
```

`gw` reads this structure from the directory where you run it. It finds the
root automatically.

Both `worktrees/` and `.bare.worktrees/` are correct. `gw` uses the directory
that exists. It does not make a second directory beside the real one.

## Requirements

- `git`
- `gh`, only for the merged-PR test in `gw clean`
- `curl`, only for the JIRA lookups

## Install

1. Run `./install.sh`. This command builds `gw` and installs it to
   `~/.local/bin`.
2. Add the shell integration to your shell profile:
   `echo 'source /path/to/gw/shell/gw.sh' >> ~/.zshrc`
3. Start a new shell.

The shell integration adds the `gwadd` function. This function makes a
worktree, then changes your directory into it.

## Where to run gw

`gw` works from these directories:

| You are in | Example |
| --- | --- |
| a worktree | `<root>/worktrees/AX-1/` |
| a subdirectory of a worktree | `<root>/worktrees/AX-1/src/` |
| the bare repository | `<root>/.bare/` |
| the layout root | `<root>/` |
| a subdirectory of the layout root | `<root>/notes/` |

`gw` goes up from the current directory until it finds the root. Thus a
subdirectory works.

`gw` does not work in a normal clone, or in a directory that is not a
repository. In this case, `gw` stops and shows you the two commands that
continue the work:

```
gw --repo <root> status          point at an existing layout
gw init <remote> --dir <root>    make a new layout
```

`gw init` is the exception. It makes a layout, so it does not need an existing
layout.

If you are not sure which repository `gw` will use, run `gw status`. The first
line shows the root that `gw` found.

## Commands

### `gw init <remote>`

Make the layout for a project. This is the only command that does not need an
existing layout.

```bash
gw init git@github.com:owner/thing.git --dir ~/src/thing
```

The command does these steps:

1. It clones a bare repository to `<root>/.bare`.
2. It configures the `refs/remotes/origin/*` refspec.
3. It checks out the default branch at `<root>/worktrees/<branch>/`.

The `--dir` value is optional. If you do not give it, `gw` uses
`./<repo-name>`. For the example above, the default is `thing`.

```bash
gw init <remote> --dir <root> --branch develop   # choose the first checkout
gw init <remote> --dir <root> --dry-run          # show the plan, write nothing
```

`gw init` does not write into an existing layout. It stops and tells you to use
`--repo`.

### `gw status`

Show each worktree with its last commit age, a dirty marker, and the JIRA
summary of the ticket in the branch name.

```
BRANCH              LAST COMMIT  DIRTY  STATE    SUMMARY
AX-47383            2d ago       ●               Add rate limiting
develop             1h ago
```

The `●` marker shows that the worktree has uncommitted changes. The `STATE`
column shows `harness`, `prunable`, or `closed` where applicable.

Use `--json` to get machine-readable output. Test scripts against the JSON
form. The spacing of the table can change.

Use `--no-jira` to skip the JIRA lookup.

### `gw add <branch>`

Make or reuse a worktree for a branch. The command has four cases:

- A worktree already exists for the branch. `gw` uses it.
- A local branch exists. `gw` moves it to `origin/<branch>`, then uses it.
- Only a remote branch exists. `gw` makes a local branch that tracks it.
- The branch does not exist. `gw` makes a new branch from `HEAD`.

The checkout directory follows the branch name. Thus `feature/x` goes to
`worktrees/feature/x`.

```bash
gwadd AX-1234                   # make or reuse the worktree, then cd into it
gw add AX-1234 --print-path     # show the path only
```

Use `--no-fetch` to skip the fetch from origin.

### `gw clean`

CAUTION: `gw clean` removes directories and deletes branches. Run
`gw clean --dry-run` first, and read the plan.

`gw clean` removes a worktree only when all of these conditions are true:

- all the commits are on a remote
- the worktree has no uncommitted changes
- the branch is not the default branch and not the repository `HEAD`
- the PR is merged, or the last commit is older than `--stale-days`

`gw` does not touch the agent worktrees in `.claude/worktrees/`.

```bash
gw clean --dry-run              # show the plan, change nothing
gw clean --yes                  # apply the plan
gw clean --closed               # also remove worktrees with a closed ticket
gw clean --delete-branches      # also delete stale local branches
gw clean --stale-days 14        # change the stale limit
gw clean --skip-github          # do not ask GitHub for merged PRs
```

`gw clean` shows the plan and changes nothing in these cases:

- you give `--dry-run`
- the output is not a terminal and you do not give `--yes`

When `gw clean` deletes a branch, it prints the tip commit and the exact
command that makes the branch again:

```
Deleted branches can be restored with:
   git branch AX-1 9df03e38ae0
```

## Configuration

All the variables are optional. `gw` finds the rest from the repository.

| Variable | Function |
| --- | --- |
| `GW_REPO` | The layout root, when you are not inside one. The same as `--repo`. |
| `GW_STALE_DAYS` | The stale limit in days. Default: 7. |
| `GW_PROTECT` | A comma-separated list of branches that `gw` must not delete. |
| `GW_JIRA_BASE_URL` | The JIRA base URL. The JIRA lookups start when you set it. |
| `GW_JIRA_USER` | The JIRA user name. `JIRA_USER` also works. |
| `GW_JIRA_API_TOKEN` | The JIRA API token. `JIRA_API_TOKEN` also works. |
| `NO_COLOR` | Disable the color and the emoji. |

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success. This includes a dry run with nothing to do. |
| 1 | An operation failed. |
| 2 | You used a command or a flag incorrectly. |
| 3 | Some of the destructive actions failed. |

## How the code is organized

The workspace has two crates. The division makes the policy testable without a
repository.

`crates/gw-core` contains the policy. It has no access to the file system, to
processes, to the network, or to the environment. It receives a snapshot of
facts and returns a plan.

`crates/gw` contains the shell. It parses the command line, runs git, GitHub,
and JIRA, and applies the plan.

The classifier matches on a `Verdict` enum with no default arm. Thus you cannot
add a new state without a decision for it.

## Development

```bash
cargo test                    # unit, property, and real-git integration tests
cargo clippy --all-targets
cargo fmt --all -- --check
```

The integration tests use real git repositories in a temporary directory. They
do not use mocks, because the behavior that matters is the behavior of git.

## License

MIT. See `LICENSE`.
