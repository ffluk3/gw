# Configuration reference

Configuration has two levels: the per-layout `.gw.toml` file, and environment
variables. Precedence is highest first:

1. command-line flag
2. environment variable
3. `.gw.toml`

## `.gw.toml`

Place it at the layout root, beside `.bare`. It is outside every checkout, so
it is local to the machine and not committed with the work.

```toml
[jira]
project = "PROJ"                                # a string...
# project = ["PROJ", "OPS"]                     # ...or a list
base_url = "https://acme.atlassian.net"
```

| Key | Type | Meaning |
| --- | --- | --- |
| `jira.project` | string or list of strings | Project prefixes a branch may reference. Empty or absent means any `[A-Z][A-Z0-9]+-[0-9]+` key. |
| `jira.base_url` | string | JIRA base URL. Must be `https://`. |

Unknown keys and syntax errors stop the command with exit code 2. Project keys
are trimmed, deduplicated case-insensitively, and empty entries are dropped.

Credentials are never read from this file.

## Environment variables

All variables are optional.

| Variable | Meaning | Default |
| --- | --- | --- |
| `GW_REPO` | Layout root, `.bare` path, `owner/name`, or unique name. Same as `--repo`. | discovery from cwd |
| `GW_CD` | Set to `1` to make a bare `gw add` change the shell's directory. Read by the shell integration, not the binary. | — |
| `GW_REPOS_ROOT` | Directory that holds layouts. A leading `~/` is expanded. | `~/repos` |
| `GW_STALE_DAYS` | Stale limit in days. | `7` |
| `GW_PROTECT` | Comma-separated branch names that must never be deleted. | — |
| `GW_JIRA_PROJECT` | Comma-separated project keys. Same as `--jira-project`. | — |
| `GW_JIRA_BASE_URL` | JIRA base URL. Same as `--jira-url`. | — |
| `GW_JIRA_USER` | JIRA user name. `JIRA_USER` is also read. | — |
| `GW_JIRA_API_TOKEN` | JIRA API token. `JIRA_API_TOKEN` is also read. | — |
| `GW_SKIP_GITHUB` | Any value skips the merged-PR lookup. Same as `--skip-github`. | — |
| `NO_COLOR` | Any value disables color and emoji. Same as `--no-color`. | — |

## Branch protection

`gw clean` always protects:

- the repository's default branch
- the branch checked out by the bare repository's `HEAD`
- every branch checked out in a worktree

`GW_PROTECT` adds more. Protected branches are never deleted. Worktree removal
is protected more narrowly: only the default branch and the bare `HEAD` are
protected from removal, because otherwise no worktree could ever be removed.

## JIRA behavior

When a base URL and both credentials are present, `gw` sends one batched JQL
request for the ticket keys it found:

```text
key in (PROJ-1,PROJ-2,...)
```

It requests `status`, `resolution`, and `summary` for up to 100 issues. A
ticket in the `done` status category counts as closed. When no resolution name
is set, the report uses `Resolved`.

Credentials travel to `curl` through a mode-0600 config file, so they do not
appear in the process list.
