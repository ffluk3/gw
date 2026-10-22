# Configuration reference

Configuration has two levels: the per-layout `.gw.toml` file, and environment
variables. Precedence is highest first:

1. command-line flag
2. environment variable
3. `.gw.toml`

## `.gw.toml`

Put the file at the layout root, beside `.bare`. It is outside each checkout,
so it is local to the machine and is not committed with the work.

```toml
[jira]
project = "PROJ"                                # a string...
# project = ["PROJ", "OPS"]                     # ...or a list
base_url = "https://acme.atlassian.net"
```

| Key | Type | Meaning |
| --- | --- | --- |
| `jira.project` | string or list of strings | Project prefixes that a branch can reference. Empty or absent means any `[A-Z][A-Z0-9]+-[0-9]+` key. |
| `jira.base_url` | string | JIRA base URL. Must be `https://`. |

Unknown keys and syntax errors stop the command with exit code 2. `gw` trims
project keys, removes duplicates without regard to case, and drops empty
entries.

`gw` never reads credentials from this file.

## Environment variables

Each variable is optional.

| Variable | Meaning | Default |
| --- | --- | --- |
| `GW_REPO` | Layout root, `.bare` path, `owner/name`, or unique name. Same as `--repo`. | discovery from the current directory |
| `GW_CD` | Set to `1` to make a bare `gw add` change the directory of the shell. The shell integration reads this variable, not the binary. | — |
| `GW_REPOS_ROOT` | Directory that holds layouts. `gw` expands a leading `~/`. | `~/repos` |
| `GW_STALE_DAYS` | Stale limit in days. | `7` |
| `GW_PROTECT` | Comma-separated branch names that `gw` must never delete. | — |
| `GW_JIRA_PROJECT` | Comma-separated project keys. Same as `--jira-project`. | — |
| `GW_JIRA_BASE_URL` | JIRA base URL. Same as `--jira-url`. | — |
| `GW_JIRA_USER` | JIRA user name. `gw` also reads `JIRA_USER`. | — |
| `GW_JIRA_API_TOKEN` | JIRA API token. `gw` also reads `JIRA_API_TOKEN`. | — |
| `GW_SKIP_GITHUB` | Each value skips the merged-PR lookup. Same as `--skip-github`. | — |
| `NO_COLOR` | Each value disables color and emoji. Same as `--no-color`. | — |

## Branch protection

`gw clean` always protects:

- the default branch of the repository
- the branch that the `HEAD` of the bare repository checks out
- each branch that a worktree checks out

`GW_PROTECT` adds more branches. `gw` never deletes a protected branch.
Worktree removal has narrower protection. Only the default branch and the bare
`HEAD` are protected from removal, because otherwise `gw` could never remove a
worktree.

## JIRA behavior

When a base URL and both credentials are present, `gw` sends one batched JQL
request for the ticket keys that it found:

```text
key in (PROJ-1,PROJ-2,...)
```

It requests `status`, `resolution`, and `summary` for up to 100 tickets. A
ticket in the `done` status category counts as closed. When no resolution name
is set, the report uses `Resolved`.

Credentials go to `curl` through a mode-0600 configuration file, so they do not
appear in the process list.
