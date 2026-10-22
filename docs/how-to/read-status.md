# Read the status table

Use this guide to understand what `gw status` prints.

## Run the command

```bash
gw status
```

```text
BRANCH              LAST COMMIT  DIRTY  STATE    SUMMARY
PROJ-47383            2d ago       ●               Add rate limiting
develop             1h ago
```

## Read each column

| Column | Meaning |
| --- | --- |
| `BRANCH` | The branch checked out in the worktree. |
| `LAST COMMIT` | The age of the newest commit in that worktree. `—` means that `gw` could not read the age. |
| `DIRTY` | `●` means uncommitted changes. An empty cell means clean. |
| `STATE` | `harness`, `prunable`, or `closed`, when applicable. |
| `SUMMARY` | The JIRA summary for the ticket key in the branch name. |

`harness` marks a worktree that the agent harness owns. `gw` never touches
these worktrees. `prunable` means that git reports that the directory is gone.

## Skip the JIRA lookup

```bash
gw status --no-jira
```

Use this option when JIRA is slow or not configured.

## Get machine-readable output

```bash
gw status --json
```

The JSON form is a stable contract. The table spacing is not. See
[Parse `gw` output in a script](script-with-json.md).

## Find the current repository

The status table does not give the name of the layout. To make sure that you
use the correct layout, run `gw repos`, which prints the root that it scans, or
pass `--repo <root>` explicitly. If discovery selects the wrong layout, `gw`
prints worktrees from a repository that you did not select.
